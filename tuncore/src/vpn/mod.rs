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
    stop_waker: Option<std::sync::Arc<::mio::Waker>>,
    exit_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    thread_join_handle: Option<std::thread::JoinHandle<()>>,
}

impl Vpn {
    pub fn new(file_descriptor: i32) -> Self {
        Self {
            file_descriptor,
            stop_waker: None,
            exit_flag: None,
            thread_join_handle: None,
        }
    }

    pub fn start(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut processor = processor::Processor::new(self.file_descriptor)?;
        self.stop_waker = Some(processor.new_stop_waker()?);
        self.exit_flag = Some(processor.exit_flag());
        self.thread_join_handle = Some(std::thread::spawn(move || processor.run().unwrap()));
        Ok(())
    }

    pub fn stop(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        self.exit_flag.as_ref().ok_or("no exit flag")?.store(true, std::sync::atomic::Ordering::Relaxed);
        self.stop_waker.as_ref().ok_or("no waker")?.wake()?;
        if let Err(e) = self.thread_join_handle.take().ok_or("no thread")?.join() {
            log::error!("failed to join thread: {:?}", e);
        }
        Ok(())
    }
}
