use crate::vpn::{
    buffers::{IncomingDataEvent, IncomingDirection, OutgoingDirection},
    session::Session,
    session_info::SessionInfo,
    utils::log_packet,
};
use mio::{event::Event, unix::SourceFd, Events, Interest, Token, Waker};
use smoltcp::time::Instant;
use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
    os::unix::io::FromRawFd,
};

type Sessions<'a> = HashMap<SessionInfo, Session<'a>>;
type TokensToSessions = HashMap<Token, SessionInfo>;

const EVENTS_CAPACITY: usize = 1024;

const TOKEN_TUN: Token = Token(0);
const TOKEN_WAKER: Token = Token(1);
const TOKEN_START_ID: usize = 10;

pub(crate) struct Processor<'a> {
    file_descriptor: i32,
    file: std::fs::File,
    poll: mio::Poll,
    sessions: Sessions<'a>,
    tokens_to_sessions: TokensToSessions,
    next_token_id: usize,
}

impl<'a> Processor<'a> {
    pub(crate) fn new(file_descriptor: i32) -> std::io::Result<Processor<'a>> {
        Ok(Processor {
            file_descriptor,
            file: unsafe { std::fs::File::from_raw_fd(file_descriptor) },
            poll: mio::Poll::new()?,
            sessions: Sessions::new(),
            tokens_to_sessions: TokensToSessions::new(),
            next_token_id: TOKEN_START_ID,
        })
    }

    pub(crate) fn new_stop_waker(&self) -> std::io::Result<Waker> {
        Waker::new(self.poll.registry(), TOKEN_WAKER)
    }

    fn generate_new_token(&mut self) -> Token {
        self.next_token_id += 1;
        Token(self.next_token_id)
    }

    pub(crate) fn run(&mut self) -> std::io::Result<()> {
        log::info!("starting vpn");

        let registry = self.poll.registry();
        registry.register(&mut SourceFd(&self.file_descriptor), TOKEN_TUN, Interest::READABLE)?;

        let mut events = Events::with_capacity(EVENTS_CAPACITY);

        'poll_loop: loop {
            if let Err(e) = self.poll.poll(&mut events, None) {
                log::debug!("failed to poll, error={:?}", e);
            }

            log::trace!("handling events, count={:?}", events.iter().count());

            for event in events.iter() {
                if event.token() == TOKEN_TUN {
                    self.handle_tun_event(event)?;
                } else if event.token() == TOKEN_WAKER {
                    log::info!("stopping vpn");
                    break 'poll_loop;
                } else {
                    self.handle_server_event(event)?;
                }
            }

            self.clearup_expired_sessions();
        }
        Ok(())
    }

    fn retrieve_or_create_session(&mut self, bytes: &[u8]) -> crate::Result<SessionInfo> {
        let session_info = SessionInfo::new(bytes)?;
        if let Some(_) = self.get_session(&session_info) {
            return Ok(session_info);
        }
        let token = self.generate_new_token();
        let session = Session::new(&session_info, &mut self.poll, token)?;
        self.tokens_to_sessions.insert(token, session_info);
        self.sessions.insert(session_info, session);
        log::debug!("created session, token={:?} session={:?}", token, session_info);
        log::debug!("sessions count={}", self.sessions.len());
        Ok(session_info)
    }

    fn destroy_session(&mut self, session_info: &SessionInfo) -> crate::Result<()> {
        log::trace!("destroying session, session={:?}", session_info);

        // push any pending data back to tun device before destroying session.
        self.write_to_smoltcp(session_info)?;
        self.write_to_tun(session_info)?;

        if let Some(session) = self.sessions.get_mut(session_info) {
            let mut smoltcp_socket = session.smoltcp_socket.get(&mut session.sockets)?;
            smoltcp_socket.close();

            let mio_socket = &mut session.mio_socket;
            if let Err(err) = mio_socket.deregister_poll(&mut self.poll) {
                log::error!("failed to deregister socket from poll, error={:?}", err);
            }
            mio_socket.close();

            let token = session.token;
            self.tokens_to_sessions.remove(&token);

            self.sessions.remove(session_info);

            log::debug!("destroyed session, token={:?} session={:?}", token, session_info);
            log::debug!("sessions count={}", self.sessions.len());
        }
        Ok(())
    }

    fn handle_tun_event(&mut self, event: &Event) -> std::io::Result<()> {
        if event.is_readable() {
            log::trace!("handle tun event");

            let mut buffer = [0_u8; crate::MAX_PACKET_SIZE];
            loop {
                let count = self.file.read(&mut buffer);
                if let Err(error) = count {
                    if error.kind() != ErrorKind::WouldBlock {
                        log::error!("failed to read from tun, error={:?}", error);
                    }
                    break;
                }
                let count = count?;
                if count == 0 {
                    break;
                }
                let read_buffer = buffer[..count].to_vec();
                log_packet("out", &read_buffer);

                let session_info = self.retrieve_or_create_session(&read_buffer);
                if let Err(error) = session_info {
                    log::info!("failed to create session, error={}", error);
                    continue;
                }
                let session_info = session_info?;
                if let Some(session) = self.get_session_mut(&session_info) {
                    session.device.receive_data(read_buffer);
                    session.update_expiry_timestamp();
                }

                self.write_to_tun(&session_info)?;
                self.read_from_smoltcp(&session_info)?;
                self.write_to_server(&session_info)?;
            }
        }
        Ok(())
    }

    fn write_to_tun(&mut self, session_info: &SessionInfo) -> crate::Result<()> {
        if let Some(session) = self.sessions.get_mut(session_info) {
            log::trace!("write to tun device, session={:?}", session_info);

            if !session.interface.poll(Instant::now(), &mut session.device, &mut session.sockets) {
                log::trace!("no readiness of socket might have changed. {:?}", session_info);
            }

            while let Some(bytes) = session.device.pop_data() {
                log_packet("in", &bytes);
                self.file.write_all(&bytes[..])?;
            }
        }
        Ok(())
    }

    fn handle_server_event(&mut self, event: &Event) -> crate::Result<()> {
        if let Some(session_info) = self.tokens_to_sessions.get(&event.token()) {
            let session_info = *session_info;

            if let Some(session) = self.get_session_mut(&session_info) {
                session.update_expiry_timestamp();
            }

            if event.is_readable() {
                log::trace!("handle server event read, session={:?}", session_info);

                self.read_from_server(&session_info)?;
                self.write_to_smoltcp(&session_info)?;
                self.write_to_tun(&session_info)?;
            }
            if event.is_writable() {
                log::trace!("handle server event write, session={:?}", session_info);

                self.read_from_smoltcp(&session_info)?;
                self.write_to_server(&session_info)?;
            }
            if event.is_read_closed() || event.is_write_closed() {
                log::trace!("handle server event closed, session={:?}", session_info);

                self.destroy_session(&session_info)?;
            }
        }
        Ok(())
    }

    fn read_from_server(&mut self, session_info: &SessionInfo) -> crate::Result<()> {
        let session = self.get_session_mut(session_info).ok_or("read_from_server")?;
        log::trace!("read from server, session={:?}", session_info);

        let (read_seqs, is_session_closed) = match session.mio_socket.read() {
            Ok(result) => result,
            Err(error) => {
                assert_ne!(error.kind(), ErrorKind::WouldBlock);
                if error.kind() != ErrorKind::ConnectionReset {
                    log::error!("failed to read from tcp stream, error={:?}", error);
                }
                (vec![], true)
            }
        };

        for bytes in read_seqs {
            if !bytes.is_empty() {
                // here exchange the business logic data
                let event = IncomingDataEvent {
                    direction: IncomingDirection::FromServer,
                    buffer: &bytes[..],
                };
                session.buffers.recv_data(event);
            }
        }

        if is_session_closed {
            self.destroy_session(session_info)?;
        }

        Ok(())
    }

    fn write_to_server(&mut self, session_info: &SessionInfo) -> crate::Result<()> {
        if let Some(session) = self.get_session_mut(session_info) {
            log::trace!("write to server, session={:?}", session_info);
            session
                .buffers
                .consume_data(OutgoingDirection::ToServer, |b| session.mio_socket.write(b).map_err(|e| e.into()));
        }
        Ok(())
    }

    fn read_from_smoltcp(&mut self, session_info: &SessionInfo) -> crate::Result<()> {
        if let Some(session) = self.get_session_mut(session_info) {
            log::trace!("read from smoltcp, session={:?}", session_info);

            let mut data = [0_u8; crate::MAX_PACKET_SIZE];
            loop {
                let mut socket = session.smoltcp_socket.get(&mut session.sockets)?;
                if !socket.can_receive() {
                    break;
                }
                let data_len = socket.receive(&mut data);
                if let Err(e) = data_len {
                    log::error!("failed to receive from smoltcp socket, error={:?}", e);
                    break;
                }
                let data_len = data_len?;
                let event = IncomingDataEvent {
                    direction: IncomingDirection::FromClient,
                    buffer: &data[..data_len],
                };
                session.buffers.recv_data(event);
            }
        }
        Ok(())
    }

    fn write_to_smoltcp(&mut self, session_info: &SessionInfo) -> crate::Result<()> {
        if let Some(session) = self.get_session_mut(session_info) {
            log::trace!("write to smoltcp, session={:?}", session_info);

            let mut socket = session.smoltcp_socket.get(&mut session.sockets)?;
            if socket.can_send() {
                session.buffers.consume_data(OutgoingDirection::ToClient, |b| socket.send(b));
            }
        }
        Ok(())
    }

    fn get_session_mut(&mut self, session_info: &SessionInfo) -> Option<&mut Session<'a>> {
        self.sessions.get_mut(session_info)
    }

    fn get_session(&self, session_info: &SessionInfo) -> Option<&Session<'a>> {
        self.sessions.get(session_info)
    }

    fn is_session_expired(&self, session_info: &SessionInfo) -> bool {
        if let Some(session) = self.get_session(session_info) {
            if let Some(expiry) = session.expiry {
                return expiry < ::std::time::Instant::now();
            }
        }
        false
    }

    fn clearup_expired_sessions(&mut self) {
        let mut expired_sessions = vec![];
        for session_info in self.sessions.keys() {
            if self.is_session_expired(session_info) {
                expired_sessions.push(session_info.clone());
            }
        }
        for session_info in expired_sessions {
            if let Err(error) = self.destroy_session(&session_info) {
                log::error!("failed to destroy session, error={:?}", error);
            }
        }
    }
}
