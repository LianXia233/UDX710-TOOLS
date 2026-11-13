//! oFono / D-Bus 适配层（报告 02/05/06 章）。
//!
//! - 使用 zbus 的 Tokio 集成，复用同一个 Tokio 运行时，不引入同步 D-Bus 阻塞调用；
//! - 每个方法单独超时，避免一次慢调用拖住整个服务；
//! - 服务不可用时降级为明确的 `ServiceUnavailable`，Handler 可返回可解释的降级结果；
//! - 提供 `reconnect` 与有界退避，避免无限重试风暴。

use std::time::Duration;
use tokio::time::timeout;
use udx_domain::error::{AppError, AppResult};
use zbus::fdo::{DBusProxy, ObjectManagerProxy, PropertiesProxy};
use zbus::names::InterfaceName;
use zbus::zvariant::Optional;
use zbus::Connection;

pub const OFONO_SERVICE: &str = "org.ofono";

#[derive(Clone)]
pub struct DbusClient {
    conn: Connection,
    timeout: Duration,
}

impl DbusClient {
    /// 连接系统总线。失败不使进程退出，允许以降级模式启动（旧版行为兼容）。
    pub async fn connect(call_timeout: Duration) -> AppResult<Self> {
        let conn = timeout(call_timeout, Connection::system())
            .await
            .map_err(|_| AppError::Timeout("连接系统 D-Bus 超时".to_string()))?
            .map_err(|e| {
                AppError::ServiceUnavailable(format!("D-Bus 连接失败: {e}"))
            })?;
        Ok(Self {
            conn,
            timeout: call_timeout,
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// 健康检查：oFono 是否已在总线上注册。
    pub async fn ofono_available(&self) -> bool {
        match timeout(self.timeout, self.list_names()).await {
            Ok(Ok(names)) => names.iter().any(|n| n == OFONO_SERVICE),
            _ => false,
        }
    }

    pub async fn list_names(&self) -> AppResult<Vec<String>> {
        let proxy = DBusProxy::new(&self.conn)
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("D-Bus 代理创建失败: {e}")))?;
        let names = timeout(self.timeout, proxy.list_names())
            .await
            .map_err(|_| AppError::Timeout("D-Bus ListNames 超时".to_string()))?
            .map_err(|e| AppError::ServiceUnavailable(format!("D-Bus ListNames 失败: {e}")))?;
        Ok(names
            .into_iter()
            .map(|n| n.as_str().to_string())
            .collect())
    }

    /// 枚举 oFono 管理的 Modem 对象路径（动态 modem 路径修复，兼容双卡切换）。
    ///
    /// **必须用 `org.ofono.Manager.GetModems`**：oFono 并不实现标准的
    /// `org.freedesktop.DBus.ObjectManager`，调用 `GetManagedObjects` 会返回
    /// `UnknownMethod`。原实现只用后者，于是永远拿不到任何 Modem 路径，
    /// 进而使状态、信号、小区、频段、IMEI 等全部模块恒为空。
    /// 这里以 GetModems 为主，ObjectManager 仅作为兼容回退。
    pub async fn ofono_modem_paths(&self) -> AppResult<Vec<String>> {
        match self.ofono_get_modems().await {
            Ok(paths) if !paths.is_empty() => Ok(paths),
            primary => match self.ofono_managed_objects().await {
                Ok(paths) if !paths.is_empty() => Ok(paths),
                _ => primary,
            },
        }
    }

    /// `org.ofono.Manager.GetModems` → `a(oa{sv})`，只取对象路径。
    async fn ofono_get_modems(&self) -> AppResult<Vec<String>> {
        let dest = zbus::names::BusName::try_from(OFONO_SERVICE)
            .map_err(|e| AppError::ServiceUnavailable(format!("D-Bus 目标非法: {e}")))?;
        let root = zbus::zvariant::ObjectPath::try_from("/")
            .map_err(|e| AppError::ServiceUnavailable(format!("D-Bus 路径非法: {e}")))?;
        let iface = InterfaceName::try_from("org.ofono.Manager")
            .map_err(|e| AppError::ServiceUnavailable(format!("接口名非法: {e}")))?;
        let member = zbus::names::MemberName::try_from("GetModems")
            .map_err(|e| AppError::ServiceUnavailable(format!("方法名非法: {e}")))?;

        let reply = timeout(
            self.timeout,
            self.conn
                .call_method(Some(dest), &root, Some(&iface), &member, &()),
        )
        .await
        .map_err(|_| AppError::Timeout("oFono GetModems 超时".to_string()))?
        .map_err(|e| AppError::ServiceUnavailable(format!("oFono GetModems 失败: {e}")))?;

        let modems: Vec<(
            zbus::zvariant::OwnedObjectPath,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        )> = reply
            .body()
            .deserialize()
            .map_err(|e| AppError::Internal(format!("oFono GetModems 响应解析失败: {e}")))?;

        Ok(modems
            .into_iter()
            .map(|(path, _)| path.as_str().to_string())
            .filter(|p| !p.is_empty())
            .collect())
    }

    /// 标准 ObjectManager 回退路径（部分 oFono 变体实现了它）。
    async fn ofono_managed_objects(&self) -> AppResult<Vec<String>> {
        let proxy = ObjectManagerProxy::builder(&self.conn)
            .destination(OFONO_SERVICE)
            .map_err(|e| AppError::ServiceUnavailable(format!("D-Bus 目标非法: {e}")))?
            .path("/")
            .map_err(|e| AppError::ServiceUnavailable(format!("D-Bus 路径非法: {e}")))?
            .build()
            .await
            .map_err(|e| {
                AppError::ServiceUnavailable(format!("oFono ObjectManager 不可用: {e}"))
            })?;

        let objects = timeout(self.timeout, proxy.get_managed_objects())
            .await
            .map_err(|_| AppError::Timeout("oFono GetManagedObjects 超时".to_string()))?
            .map_err(|e| {
                AppError::ServiceUnavailable(format!("oFono 对象枚举失败: {e}"))
            })?;

        Ok(objects
            .keys()
            .map(|p| p.as_str().to_string())
            .filter(|p| !p.is_empty())
            .collect())
    }

    /// 读取 oFono 接口的全部属性，返回 JSON（避免手工拼 JSON）。
    ///
    /// 本机的 oFono（UNISOC 定制）**不实现**标准的
    /// `org.freedesktop.DBus.Properties.GetAll`（实测返回 UnknownMethod），
    /// 而是在每个业务接口上提供 `<Iface>.GetProperties`。因此优先走自定义
    /// 方法，失败再回退标准 GetAll —— 只走标准路径会永远拿不到属性。
    pub async fn ofono_properties(
        &self,
        modem_path: &str,
        interface: &str,
    ) -> AppResult<serde_json::Value> {
        if let Ok(props) = self
            .ofono_call(modem_path, interface, "GetProperties")
            .await
        {
            if props
                .as_object()
                .map(|m| !m.is_empty())
                .unwrap_or(false)
            {
                return Ok(props);
            }
        }

        let proxy = PropertiesProxy::builder(&self.conn)
            .destination(OFONO_SERVICE)
            .map_err(|e| AppError::ServiceUnavailable(format!("D-Bus 目标非法: {e}")))?
            .path(modem_path)
            .map_err(|e| AppError::ServiceUnavailable(format!("modem 路径非法: {e}")))?
            .build()
            .await
            .map_err(|e| {
                AppError::ServiceUnavailable(format!("oFono Properties 不可用: {e}"))
            })?;

        let iface = InterfaceName::try_from(interface)
            .map_err(|e| AppError::ServiceUnavailable(format!("接口名非法: {e}")))?;
        let props = timeout(self.timeout, proxy.get_all(Optional::from(Some(iface))))
            .await
            .map_err(|_| AppError::Timeout(format!("oFono GetAll({interface}) 超时")))?
            .map_err(|e| {
                AppError::ServiceUnavailable(format!("oFono GetAll 失败: {e}"))
            })?;

        let raw = serde_json::to_value(&props)
            .map_err(|e| AppError::Internal(format!("D-Bus 属性序列化失败: {e}")))?;
        Ok(zvariant_unwrap(&raw))
    }

    /// 调用 oFono 上不带参数的接口方法，返回 `a{sv}` 形式的响应。
    ///
    /// 用于 `org.ofono.NetworkMonitor.GetServingCellInformation` —— 这是获取
    /// RSRP / RSRQ / PCI / ARFCN 的标准来源；标准的 `NetworkRegistration`
    /// 只提供 Strength（0-100），无法满足信号格展示。
    pub async fn ofono_call(
        &self,
        modem_path: &str,
        interface: &str,
        method: &str,
    ) -> AppResult<serde_json::Value> {
        let dest = zbus::names::BusName::try_from(OFONO_SERVICE)
            .map_err(|e| AppError::ServiceUnavailable(format!("D-Bus 目标非法: {e}")))?;
        let path = zbus::zvariant::ObjectPath::try_from(modem_path)
            .map_err(|e| AppError::ServiceUnavailable(format!("modem 路径非法: {e}")))?;
        let iface = InterfaceName::try_from(interface)
            .map_err(|e| AppError::ServiceUnavailable(format!("接口名非法: {e}")))?;
        let member = zbus::names::MemberName::try_from(method)
            .map_err(|e| AppError::ServiceUnavailable(format!("方法名非法: {e}")))?;

        let reply = timeout(
            self.timeout,
            self.conn
                .call_method(Some(dest), &path, Some(&iface), &member, &()),
        )
        .await
        .map_err(|_| AppError::Timeout(format!("oFono {interface}.{method} 超时")))?
        .map_err(|e| {
            AppError::ServiceUnavailable(format!("oFono {interface}.{method} 失败: {e}"))
        })?;

        let data: std::collections::HashMap<String, zbus::zvariant::OwnedValue> = reply
            .body()
            .deserialize()
            .map_err(|e| AppError::Internal(format!("D-Bus 响应解析失败: {e}")))?;
        let raw = serde_json::to_value(&data)
            .map_err(|e| AppError::Internal(format!("D-Bus 响应序列化失败: {e}")))?;
        Ok(zvariant_unwrap(&raw))
    }

    /// 断线恢复：重新建立系统总线连接。调用方负责有界退避与重试次数上限。
    pub async fn reconnect(&self) -> AppResult<Self> {
        Self::connect(self.timeout).await
    }
}

/// 把 zvariant 的序列化表示还原成普通 JSON。
///
/// `zvariant::Value` 的 `Serialize` 为了携带类型签名，输出的是
/// `{"zvariant::Value::Signature":"s","zvariant::Value::Value":"CMCC"}`
/// 这样的包装对象。若直接当业务字段使用，取到的是对象而不是字符串，
/// 表现为「运营商 / 型号 / 序列号全部为空」而 D-Bus 侧其实有值。
/// 这里递归剥掉包装，保留真实值。
fn zvariant_unwrap(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(zvariant_unwrap).collect())
        }
        serde_json::Value::Object(map) => {
            if let Some(inner) = map.get("zvariant::Value::Value") {
                return zvariant_unwrap(inner);
            }
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), zvariant_unwrap(v));
            }
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    }
}
