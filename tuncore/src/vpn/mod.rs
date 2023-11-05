mod buffers;
mod mio_socket;
mod processor;
mod session;
mod session_info;
mod smoltcp_socket;
mod utils;
mod vpn_device;

pub(super) struct Vpn {
    file_descriptor: i32,
    stop_waker: Option<::mio::Waker>,
    thread_join_handle: Option<std::thread::JoinHandle<()>>,
}

impl Vpn {
    pub fn new(file_descriptor: i32) -> Self {
        Self {
            file_descriptor,
            stop_waker: None,
            thread_join_handle: None,
        }
    }

    pub fn start(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut processor = processor::Processor::new(self.file_descriptor)?;
        self.stop_waker = Some(processor.new_stop_waker()?);
        self.thread_join_handle = Some(std::thread::spawn(move || processor.run().unwrap()));
        Ok(())
    }

    pub fn stop(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        self.stop_waker.as_ref().ok_or("no waker")?.wake()?;
        if let Err(e) = self.thread_join_handle.take().ok_or("no thread")?.join() {
            log::error!("failed to join thread: {:?}", e);
        }
        Ok(())
    }
}
