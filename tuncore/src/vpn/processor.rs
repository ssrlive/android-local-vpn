use crate::vpn::{session::Session, session_info::SessionInfo};
#[cfg(target_family = "unix")]
use mio::unix::SourceFd;
use mio::{event::Event, Events, Interest, Token, Waker};
#[cfg(target_family = "unix")]
use std::os::unix::io::FromRawFd;
use std::{
    collections::HashMap,
    io::{ErrorKind, Read},
};

type SessionHashMap<'a> = HashMap<SessionInfo, Session<'a>>;

const EVENTS_CAPACITY: usize = 1024;

const TOKEN_TUN: Token = Token(0);
const TOKEN_WAKER: Token = Token(1);
const TOKEN_START_ID: usize = 10;

pub(crate) struct Processor<'a> {
    #[cfg(target_family = "unix")]
    file_descriptor: i32,
    #[cfg(target_family = "unix")]
    file: std::fs::File,
    poll: mio::Poll,
    sessions: SessionHashMap<'a>,
    next_token_id: usize,
    waker: Option<std::sync::Arc<::mio::Waker>>,
    exit_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl<'a> Processor<'a> {
    pub(crate) fn new(file_descriptor: i32) -> std::io::Result<Processor<'a>> {
        Ok(Processor {
            #[cfg(target_family = "unix")]
            file_descriptor,
            #[cfg(target_family = "unix")]
            file: unsafe { std::fs::File::from_raw_fd(file_descriptor) },
            poll: mio::Poll::new()?,
            sessions: SessionHashMap::new(),
            next_token_id: TOKEN_START_ID,
            waker: None,
            exit_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    pub(crate) fn exit_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.exit_flag.clone()
    }

    pub(crate) fn new_stop_waker(&mut self) -> std::io::Result<std::sync::Arc<Waker>> {
        self.create_stop_waker()?;
        Ok(self.waker.clone().unwrap())
    }

    fn create_stop_waker(&mut self) -> std::io::Result<()> {
        if self.waker.is_none() {
            self.waker = Some(std::sync::Arc::new(Waker::new(self.poll.registry(), TOKEN_WAKER)?));
        }
        Ok(())
    }

    fn generate_new_token(&mut self) -> Token {
        self.next_token_id += 1;
        Token(self.next_token_id)
    }

    pub(crate) fn run(&mut self) -> std::io::Result<()> {
        log::info!("starting vpn");

        #[cfg(target_family = "unix")]
        let registry = self.poll.registry();
        #[cfg(target_family = "unix")]
        registry.register(&mut SourceFd(&self.file_descriptor), TOKEN_TUN, Interest::READABLE | Interest::WRITABLE)?;

        let mut events = Events::with_capacity(EVENTS_CAPACITY);
        let timeout = Some(std::time::Duration::from_secs(crate::POLL_TIMEOUT));

        self.create_stop_waker()?;

        'poll_loop: loop {
            if let Err(e) = self.poll.poll(&mut events, timeout) {
                log::debug!("failed to poll, error={:?}", e);
            }

            log::trace!("handling events, count={:?}", events.iter().count());

            for event in events.iter() {
                if event.token() == TOKEN_TUN {
                    self.handle_tun_event(event)?;
                } else if event.token() == TOKEN_WAKER {
                    if self.exit_flag.load(std::sync::atomic::Ordering::Relaxed) {
                        log::info!("stopping vpn");
                        break 'poll_loop;
                    }
                } else {
                    self.handle_server_event(event)?;
                }
            }

            self.clearup_expired_sessions();
            log::trace!("sessions count={}", self.sessions.len());
        }
        Ok(())
    }

    fn retrieve_or_create_session(&mut self, bytes: &[u8], is_closed: &mut bool) -> crate::Result<SessionInfo> {
        let session_info = SessionInfo::new(bytes, is_closed)?;
        if self.sessions.get(&session_info).is_some() {
            return Ok(session_info);
        }
        let token = self.generate_new_token();
        let session = Session::new(&session_info, &mut self.poll, token)?;
        self.sessions.insert(session_info, session);
        log::debug!("created session, {:?} {:?}", token, session_info);
        Ok(session_info)
    }

    fn destroy_session(&mut self, session_info: &SessionInfo) -> crate::Result<()> {
        if let Some(mut session) = self.sessions.remove(session_info) {
            // push any pending data back to tun device before destroying session.
            session.write_to_smoltcp()?;

            #[cfg(target_family = "unix")]
            session.write_to_tun(&mut self.file)?;
            #[cfg(target_family = "windows")]
            assert!(false, "windows not supported yet");

            session.destroy(&mut self.poll)?;
            log::debug!("destroyed session, {:?} {:?}", session.token, session_info);
        }
        Ok(())
    }

    fn handle_tun_event(&mut self, event: &Event) -> std::io::Result<()> {
        if event.is_readable() {
            log::trace!("handle tun event");

            let mut buffer = [0_u8; crate::MAX_PACKET_SIZE];
            loop {
                #[cfg(target_family = "unix")]
                let count = self.file.read(&mut buffer);
                #[cfg(target_family = "windows")]
                let count: Result<usize, std::io::Error> = Ok(0_usize);
                #[cfg(target_family = "windows")]
                assert!(false, "windows not supported yet");
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

                let mut is_closed = false;
                let session_info = self.retrieve_or_create_session(&read_buffer, &mut is_closed);
                if let Err(error) = session_info {
                    log::info!("failed to create session, error={}", error);
                    continue;
                }
                let session_info = session_info?;
                if let Some(session) = self.sessions.get_mut(&session_info) {
                    session.store_tun_data(read_buffer);

                    #[cfg(target_family = "unix")]
                    session.write_to_tun(&mut self.file)?;
                    #[cfg(target_family = "windows")]
                    assert!(false, "windows not supported yet");

                    session.read_from_smoltcp()?;
                    session.write_to_server(&mut is_closed)?;

                    // delay tcp socket close to avoid RST packet
                    session.update_expiry_timestamp(is_closed);
                }
            }
        }
        if event.is_writable() {
            let targets = self.sessions.iter().filter(|(_, s)| s.continue_read()).map(|(i, _)| *i).collect::<Vec<_>>();
            for session_info in targets {
                let mut is_closed = false;
                self.read_server_n_write_client(session_info, &mut is_closed)?;
            }
        }
        Ok(())
    }

    fn read_server_n_write_client(&mut self, session_info: SessionInfo, is_closed: &mut bool) -> crate::Result<()> {
        if let Some(session) = self.sessions.get_mut(&session_info) {
            let mut _is_closed = false;
            session.read_from_server(&mut _is_closed)?;
            session.write_to_smoltcp()?;

            #[cfg(target_family = "unix")]
            session.write_to_tun(&mut self.file)?;
            #[cfg(target_family = "windows")]
            assert!(false, "windows not supported yet");

            session.update_expiry_timestamp(_is_closed);
            *is_closed = _is_closed;
        }
        Ok(())
    }

    fn handle_server_event(&mut self, event: &Event) -> crate::Result<()> {
        if let Some((session_info, _)) = self.sessions.iter().find(|(_, session)| session.token == event.token()) {
            let session_info = *session_info;

            let mut is_closed = false;
            if event.is_readable() {
                log::trace!("handle server event read, {:?}", session_info);

                self.read_server_n_write_client(session_info, &mut is_closed)?;
            }
            if event.is_writable() {
                log::trace!("handle server event write, {:?}", session_info);

                if let Some(session) = self.sessions.get_mut(&session_info) {
                    session.read_from_smoltcp()?;
                    session.write_to_server(&mut is_closed)?;
                }
            }
            let force_set = event.is_read_closed() || event.is_write_closed() || is_closed;
            if let Some(session) = self.sessions.get_mut(&session_info) {
                session.update_expiry_timestamp(force_set);
            }
            if force_set {
                // since the session is closed by server, we can destroy it immediately.
                if let Err(error) = self.destroy_session(&session_info) {
                    log::error!("failed to destroy session, error={:?}", error);
                }
            }
        }
        Ok(())
    }

    fn clearup_expired_sessions(&mut self) {
        let expired_sessions = self.sessions.iter().filter(|(_, s)| s.is_expired()).map(|(i, _)| *i).collect::<Vec<_>>();
        for session_info in expired_sessions {
            if let Err(error) = self.destroy_session(&session_info) {
                log::error!("failed to destroy session, error={:?}", error);
            }
        }
    }
}
