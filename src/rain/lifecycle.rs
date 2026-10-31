//! Event-driven power/session observations, only subscribed while rain is on.
use cosmic::iced::{
    Subscription,
    futures::{SinkExt, Stream, StreamExt},
};
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub ready: bool,
    pub locked: bool,
    pub sleeping: bool,
    pub on_battery: bool,
}
pub fn watch() -> Subscription<State> {
    Subscription::run(feed)
}
fn feed() -> impl Stream<Item = State> {
    cosmic::iced::stream::channel(1, async move |mut out| {
        loop {
            let _result = async {
                let bus = zbus::Connection::system().await?;
                let manager = zbus::Proxy::new(
                    &bus,
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                )
                .await?;
                // The widget service runs in the user's app slice, which need
                // not belong to a logind session. Resolve the graphical Display
                // session too; PID-only lookup would silently disable rain.
                let path: zbus::zvariant::OwnedObjectPath = match manager
                    .call("GetSessionByPID", &(std::process::id(),))
                    .await
                {
                    Ok(path) => path,
                    Err(_) => {
                        let user_path: zbus::zvariant::OwnedObjectPath = manager
                            .call("GetUser", &(unsafe { libc::geteuid() },))
                            .await?;
                        let user = zbus::Proxy::new(
                            &bus,
                            "org.freedesktop.login1",
                            user_path,
                            "org.freedesktop.login1.User",
                        )
                        .await?;
                        let (_, path): (String, zbus::zvariant::OwnedObjectPath) =
                            user.get_property("Display").await?;
                        path
                    }
                };
                let session = zbus::Proxy::new(
                    &bus,
                    "org.freedesktop.login1",
                    path,
                    "org.freedesktop.login1.Session",
                )
                .await?;
                let power = zbus::Proxy::new(
                    &bus,
                    "org.freedesktop.UPower",
                    "/org/freedesktop/UPower",
                    "org.freedesktop.UPower",
                )
                .await?;
                let mut session_owner = manager.receive_owner_changed().await?;
                let mut power_owner = power.receive_owner_changed().await?;
                let mut lock = session.receive_property_changed::<bool>("LockedHint").await;
                let mut sleep = manager.receive_signal("PrepareForSleep").await?;
                let mut battery = power.receive_property_changed::<bool>("OnBattery").await;
                let mut state = State {
                    ready: true,
                    locked: session.get_property("LockedHint").await?,
                    sleeping: manager.get_property("PreparingForSleep").await?,
                    on_battery: power.get_property("OnBattery").await?,
                };
                if out.send(state).await.is_err() {
                    return Ok::<(), zbus::Error>(());
                }
                loop {
                    tokio::select! {
                        _=session_owner.next()=>return Ok(()),
                        _=power_owner.next()=>return Ok(()),
                        Some(v)=lock.next()=>state.locked=v.get().await?,
                        Some(v)=sleep.next()=>state.sleeping=v.body().deserialize::<bool>()?,
                        Some(v)=battery.next()=>state.on_battery=v.get().await?,
                        else=>return Ok(()),
                    }
                    if out.send(state).await.is_err() {
                        return Ok(());
                    }
                }
            }
            .await;
            // A disappeared service or bus is also an unknown session. Drop
            // all animated surfaces before attempting to reconnect.
            if out.send(State::default()).await.is_err() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    })
}
