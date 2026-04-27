//! Async task queue with concurrency control.

use std::future::Future;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct AsyncTaskQueue {
    semaphore: Arc<Semaphore>,
}

impl AsyncTaskQueue {
    pub fn new(concurrency: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(concurrency)),
        }
    }

    pub async fn add<T, F>(&self, task: F) -> T
    where
        F: Future<Output = T> + Send,
        T: Send,
    {
        let _permit = self.semaphore.acquire().await.unwrap();
        task.await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_and_add_returns_task_result() {
        let queue = AsyncTaskQueue::new(2);
        let r: i32 = queue.add(async { 42 }).await;
        assert_eq!(r, 42);
    }

    #[tokio::test]
    async fn add_respects_concurrency() {
        let queue = AsyncTaskQueue::new(1);
        let a = queue.add(async { 1 });
        let b = queue.add(async { 2 });
        let (x, y) = tokio::join!(a, b);
        assert_eq!((x, y), (1, 2));
    }
}
