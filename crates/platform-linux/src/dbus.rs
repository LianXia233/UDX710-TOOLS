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
    pub async fn ofono_modem_paths(&self) -> AppResult<Vec<String>> {
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
    pub async fn ofono_properties(
        &self,
        modem_path: &str,
        interface: &str,
    ) -> AppResult<serde_json::Value> {
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

        serde_json::to_value(&props)
            .map_err(|e| AppError::Internal(format!("D-Bus 属性序列化失败: {e}")))
    }

    /// 断线恢复：重新建立系统总线连接。调用方负责有界退避与重试次数上限。
    pub async fn reconnect(&self) -> AppResult<Self> {
        Self::connect(self.timeout).await
    }
}
