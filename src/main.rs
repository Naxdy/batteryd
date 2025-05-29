use std::{collections::HashMap, time::Duration};

use futures::{StreamExt, stream::FuturesUnordered};
use tokio::time::sleep;
use upower_dbus::{DeviceProxy, UPowerProxy};
use zbus::{dbus_proxy as proxy, zvariant::Value};

const CRIT_PERCENTAGE: f64 = 20.;

struct DeviceInfo<'d> {
    proxy: DeviceProxy<'d>,
    last_percentage: f64,
}

impl<'d> DeviceInfo<'d> {
    fn new(proxy: DeviceProxy<'d>) -> Self {
        Self {
            last_percentage: 100.,
            proxy,
        }
    }
}

#[proxy(
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    /// Call the org.freedesktop.Notifications.Notify D-Bus method
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, &Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;
}

#[tokio::main]
async fn main() -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;

    let upower = UPowerProxy::new(&connection).await?;

    let mut devices = upower
        .enumerate_devices()
        .await?
        .into_iter()
        .map(async |e| Ok(DeviceInfo::new(DeviceProxy::new(&connection, e).await?)))
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, zbus::Error>>()?;

    let session = zbus::Connection::session().await?;

    let notif = NotificationsProxy::new(&session).await?;

    let sound_name = zbus::zvariant::Value::new("battery-caution");
    let urgency = zbus::zvariant::Value::new(2);

    loop {
        devices
            .iter_mut()
            .map(|e| async {
                let percentage = e.proxy.percentage().await?;
                if percentage < e.last_percentage && percentage <= CRIT_PERCENTAGE {
                    let mut hint_map = HashMap::new();
                    hint_map.insert("sound-name", &sound_name);
                    hint_map.insert("urgency", &urgency);

                    notif
                        .notify(
                            "batteryd",
                            0,
                            "battery-caution",
                            "Battery Low",
                            &format!(
                                "The device {} has reached {}% battery.",
                                e.proxy.model().await?,
                                percentage
                            ),
                            &[],
                            hint_map,
                            5000,
                        )
                        .await?;

                    e.last_percentage = percentage;
                }

                Ok::<(), zbus::Error>(())
            })
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        sleep(Duration::from_secs(60)).await
    }
}
