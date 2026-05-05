use std::{
    pin::Pin,
    task::{Context, Poll},
};

pub struct PeriodicYielder {
    count: u32,
    period: u32,
}

impl Default for PeriodicYielder {
    fn default() -> Self {
        Self::new(16384)
    }
}

impl PeriodicYielder {
    pub fn new(period: u32) -> Self {
        Self { count: 0, period }
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }

    pub async fn tick(&mut self) {
        self.count += 1;
        if self.count >= self.period {
            self.count = 0;

            yield_now().await;
        }
    }
}

// https://users.rust-lang.org/t/what-is-the-asynchronous-version-of-yield-now/24367
pub async fn yield_now() {
    YieldNow(false).await
}

#[derive(Default)]
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
