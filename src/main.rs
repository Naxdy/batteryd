use std::{collections::HashMap, time::Duration};

use futures::{StreamExt, stream::FuturesUnordered};
use tokio::time::sleep;
use upower_dbus::{DeviceProxy, UPowerProxy};
use zbus::{dbus_proxy as proxy, zvariant::Value};

const CRIT_PERCENTAGE: f64 = 20.;

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

    let session = zbus::Connection::session().await?;

    let notif = NotificationsProxy::new(&session).await?;

    let sound_name = zbus::zvariant::Value::new("battery-caution");
    let urgency = zbus::zvariant::Value::new(2);

    let mut last_percentages = HashMap::new();

    loop {
        let devices = upower
            .enumerate_devices()
            .await?
            .into_iter()
            .map(|e| DeviceProxy::new(&connection, e))
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, zbus::Error>>()?;

        let percentage_infos = devices
            .iter()
            .map(async |e| Ok((e.path().to_string(), e.percentage().await?)))
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, zbus::Error>>()?;

        percentage_infos.into_iter().for_each(|e| {
            last_percentages.insert(e.0, e.1);
        });

        devices
            .iter()
            .map(async |e| {
                let last_percentage = *last_percentages.get(&e.path().to_string()).unwrap_or(&100.);
                let percentage = e.percentage().await?;

                if percentage < last_percentage && percentage <= CRIT_PERCENTAGE {
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
                                e.model().await?,
                                percentage
                            ),
                            &[],
                            hint_map,
                            5000,
                        )
                        .await?;
                }

                Ok(())
            })
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, zbus::Error>>()?;

        sleep(Duration::from_secs(60)).await;
    }
}
