use crate::Error;
use iced_wgpu::wgpu;
use std::{sync::mpsc, thread};

/// Sleeps until a submission exists, then waits for precisely that submission.
/// Polling and joining never hold the shared queue gate or the pool mutex.
pub(crate) struct Completion {
    sender: Option<mpsc::Sender<(wgpu::SubmissionIndex, usize)>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Completion {
    pub fn new(
        device: wgpu::Device,
        completed: impl Fn(usize, Result<(), String>) + Send + 'static,
    ) -> Result<Self, Error> {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("mpv-gpu-completion".into())
            .spawn(move || {
                while let Ok((submission, token)) = receiver.recv() {
                    let result = device
                        .poll(wgpu::PollType::Wait {
                            submission_index: Some(submission),
                            timeout: None,
                        })
                        .map(|_| ())
                        .map_err(|error| format!("embedded MPV GPU completion failed: {error}"));
                    completed(token, result);
                }
            })
            .map_err(|error| {
                Error::from(format!("cannot start MPV GPU completion worker: {error}"))
            })?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
        })
    }

    pub fn track(&self, submission: wgpu::SubmissionIndex, token: usize) -> Result<(), Error> {
        self.sender
            .as_ref()
            .ok_or("MPV GPU completion worker is stopped")?
            .send((submission, token))
            .map_err(|_| Error::from("MPV GPU completion worker exited"))
    }

    pub fn shutdown(&mut self) -> Result<(), Error> {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| Error::from("MPV GPU completion worker panicked"))?;
        }
        Ok(())
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
