//! Bridges the tokio runtime (network, disk) with the GTK main loop.

use gtk::glib;
use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

pub fn rt() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("quran-io")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

/// Run `fut` on the IO runtime and hand its output to `done` on the GTK main thread.
pub fn spawn<T, F>(fut: F, done: impl FnOnce(T) + 'static)
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    let handle = rt().spawn(fut);
    glib::spawn_future_local(async move {
        if let Ok(v) = handle.await {
            done(v);
        }
    });
}

/// Fire-and-forget background work.
pub fn spawn_bg<F>(fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    rt().spawn(fut);
}
