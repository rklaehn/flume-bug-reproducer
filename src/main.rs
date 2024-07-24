use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::{
    sync::Semaphore,
    task::{JoinSet, LocalSet},
};

type BoxedFut<T = ()> = Pin<Box<dyn Future<Output = T>>>;
type SpawnFn<T = ()> = Box<dyn FnOnce() -> BoxedFut<T> + Send + 'static>;

enum Message {
    Execute(SpawnFn),
    Finish,
}

pub struct LocalPool {
    shutdown_sem: Arc<Semaphore>,
    send: flume::Sender<Message>,
    threads: u32,
}

impl LocalPool {
    pub fn new() -> Self {
        let threads = num_cpus::get() as u32;
        let (send, recv) = flume::unbounded::<Message>();
        let shutdown_sem = Arc::new(Semaphore::new(0));
        let handle = tokio::runtime::Handle::current();
        for _ in 0..threads {
            Self::spawn_pool_thread(recv.clone(), shutdown_sem.clone(), handle.clone());
        }
        Self {
            send,
            shutdown_sem,
            threads,
        }
    }

    fn spawn_pool_thread(
        recv: flume::Receiver<Message>,
        shutdown_sem: Arc<Semaphore>,
        handle: tokio::runtime::Handle,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let mut s = JoinSet::new();
            let ls = LocalSet::new();
            handle.block_on(ls.run_until(async {
                loop {
                    tokio::select! {
                        _ = s.join_next(), if !s.is_empty() => {},
                        msg = recv.recv_async() => {
                            match msg {
                                Ok(Message::Execute(f)) => {
                                    s.spawn_local((f)());
                                }
                                Ok(Message::Finish) => break,
                                Err(_) => break,
                            }
                        },
                    }
                }
            }));
            // somebody is asking for a clean shutdown, wait for all tasks to finish
            handle.block_on(ls.run_until(async { while let Some(_) = s.join_next().await {} }));
            shutdown_sem.add_permits(1);
        })
    }

    pub async fn finish(self) {
        for _ in 0..self.threads {
            self.send.send_async(Message::Finish).await.ok();
        }
        let _ = self.shutdown_sem.acquire_many(self.threads).await.unwrap();
    }

    pub fn spawn_detached<F, Fut>(&self, gen: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        let gen: SpawnFn = Box::new(move || Box::pin(gen()));
        self.send.send(Message::Execute(gen)).unwrap();
    }
}

#[tokio::main]
async fn main() {
    let pool = LocalPool::new();
    let n = 4;
    for _ in 0..n {
        pool.spawn_detached(move || async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
        });
    }
    pool.finish().await;
    let t = std::time::SystemTime::now();
    println!("{:?}", t);
}
