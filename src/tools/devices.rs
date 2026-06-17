use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Empty, Id, flute_err_to_result, value_to_result};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceRegister {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceId {
    pub device_id: String,
}

#[tool_router(router = devices_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "List mobile payment devices. Records use `deviceId`, not `id`. Safe to retry.")]
    pub async fn devices_list(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["devices".into(), "list".into()]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Get a device by id. Safe to retry.")]
    pub async fn devices_get(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["devices".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Mint a Tap-to-Pay JWT for a device. Returns a `tap_to_pay_jwt` envelope. Idempotent read.")]
    pub async fn devices_ttp_jwt(&self, Parameters(p): Parameters<DeviceId>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["devices".into(), "ttp-jwt".into(), "--device-id".into(), p.device_id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Register a device. NOT idempotent. Optional --name.")]
    pub async fn devices_register(&self, Parameters(p): Parameters<DeviceRegister>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("devices_register") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["devices".into(), "register".into(), p.id]);
        if let Some(v) = p.name { args.extend(["--name".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Activate Tap-to-Pay on a device. NOT idempotent.")]
    pub async fn devices_ttp_activate(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("devices_ttp_activate") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["devices".into(), "ttp-activate".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
