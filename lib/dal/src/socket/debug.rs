use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use telemetry::prelude::*;
use thiserror::Error;
use ulid::Ulid;

use super::{input::InputSocketError, output::OutputSocketError};
use crate::{
    attribute::{
        prototype::{
            debug::{AttributePrototypeDebugView, AttributePrototypeDebugViewError},
            AttributePrototypeError,
        },
        value::AttributeValueError,
    },
    func::execution::FuncExecution,
    AttributePrototype, AttributePrototypeId, AttributeValue, AttributeValueId, DalContext, FuncId,
    InputSocket, InputSocketId, OutputSocket, OutputSocketId,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocketDebugView {
    pub path: String,
    pub socket_id: Ulid,
    pub attribute_value_id: AttributeValueId,
    pub func_id: FuncId,
    pub func_execution: Option<FuncExecution>,
    pub prototype_id: Option<AttributePrototypeId>,
    pub connection_annotations: Vec<String>,
    pub func_name: String,
    pub func_args: HashMap<String, Vec<serde_json::Value>>,
    pub arg_sources: HashMap<String, Option<String>>,
    pub value: Option<serde_json::Value>,
    pub materialized_view: Option<serde_json::Value>,
    pub name: String,
}
type SocketDebugViewResult<T> = Result<T, SocketDebugViewError>;

#[remain::sorted]
#[derive(Error, Debug)]
pub enum SocketDebugViewError {
    #[error("attribute prototype debug view error: {0}")]
    AttributePrototypeDebugViewError(#[from] AttributePrototypeDebugViewError),
    #[error("attribute prototype error: {0}")]
    AttributePrototypeError(#[from] AttributePrototypeError),
    #[error("attribute value error: {0}")]
    AttributeValue(#[from] AttributeValueError),
    #[error("input socket error: {0}")]
    InputSocketError(#[from] InputSocketError),
    #[error("output socket error: {0}")]
    OutputSocketError(#[from] OutputSocketError),
}

impl SocketDebugView {
    #[instrument(level = "info", skip_all)]
    pub async fn new_for_output_socket(
        ctx: &DalContext,
        output_socket_id: OutputSocketId,
    ) -> SocketDebugViewResult<SocketDebugView> {
        let prototype_id =
            AttributePrototype::find_for_output_socket(ctx, output_socket_id).await?;

        let attribute_value_id =
            OutputSocket::attribute_values_for_output_socket_id(ctx, output_socket_id)
                .await?
                .pop()
                .expect("should have attribute value id");

        let prototype_debug_view =
            AttributePrototypeDebugView::assemble(ctx, attribute_value_id).await?;
        let attribute_value = AttributeValue::get_by_id(ctx, attribute_value_id).await?;
        let output_socket = OutputSocket::get_by_id(ctx, output_socket_id).await?;
        let connection_annotations = output_socket
            .connection_annotations()
            .into_iter()
            .map(|f| f.to_string())
            .collect();
        let path = match AttributeValue::get_path_for_id(ctx, attribute_value_id).await? {
            Some(path) => path,
            None => String::new(),
        };

        let materialized_view = attribute_value.materialized_view(ctx).await?;

        Ok(SocketDebugView {
            prototype_id,
            func_name: prototype_debug_view.func_name,
            func_args: prototype_debug_view.func_args,
            arg_sources: prototype_debug_view.arg_sources,
            attribute_value_id,
            socket_id: output_socket_id.into(),
            func_id: prototype_debug_view.func_id,
            func_execution: prototype_debug_view.func_execution,
            connection_annotations,
            value: attribute_value.unprocessed_value(ctx).await?,
            path,
            materialized_view,
            name: output_socket.name().to_string(),
        })
    }
    #[instrument(level = "info", skip_all)]
    pub async fn new_for_input_socket(
        ctx: &DalContext,
        input_socket_id: InputSocketId,
    ) -> SocketDebugViewResult<SocketDebugView> {
        let prototype_id = AttributePrototype::find_for_input_socket(ctx, input_socket_id).await?;
        let attribute_value_id =
            InputSocket::attribute_values_for_input_socket_id(ctx, input_socket_id)
                .await?
                .pop()
                .expect("should have attribute value id");
        let prototype_debug_view =
            AttributePrototypeDebugView::assemble(ctx, attribute_value_id).await?;
        let attribute_value = AttributeValue::get_by_id(ctx, attribute_value_id).await?;
        let input_socket = InputSocket::get_by_id(ctx, input_socket_id).await?;
        let connection_annotations = input_socket
            .connection_annotations()
            .into_iter()
            .map(|f| f.to_string())
            .collect();
        let path = match AttributeValue::get_path_for_id(ctx, attribute_value_id).await? {
            Some(path) => path,
            None => String::new(),
        };
        let materialized_view = attribute_value.materialized_view(ctx).await?;

        Ok(SocketDebugView {
            prototype_id,
            func_name: prototype_debug_view.func_name,
            func_args: prototype_debug_view.func_args,
            arg_sources: prototype_debug_view.arg_sources,
            attribute_value_id,
            socket_id: input_socket_id.into(),
            func_id: prototype_debug_view.func_id,
            func_execution: prototype_debug_view.func_execution,
            connection_annotations,
            value: attribute_value.unprocessed_value(ctx).await?,
            path,
            materialized_view,
            name: input_socket.name().to_string(),
        })
    }
}
