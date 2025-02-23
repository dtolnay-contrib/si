use axum::{response::IntoResponse, Json};
use serde::{Deserialize, Serialize};

use dal::{AttributeValue, AttributeValueId, ChangeSet, Visibility};

use crate::server::extract::{AccessBuilder, HandlerContext};

use super::ComponentResult;

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RestoreDefaultFunctionRequest {
    pub attribute_value_id: AttributeValueId,
    #[serde(flatten)]
    pub visibility: Visibility,
}

pub async fn restore_default_function(
    HandlerContext(builder): HandlerContext,
    AccessBuilder(request_ctx): AccessBuilder,
    Json(request): Json<RestoreDefaultFunctionRequest>,
) -> ComponentResult<impl IntoResponse> {
    let mut ctx = builder.build(request_ctx.build(request.visibility)).await?;

    let force_change_set_id = ChangeSet::force_new(&mut ctx).await?;

    AttributeValue::use_default_prototype(&ctx, request.attribute_value_id).await?;

    // Track
    // {
    //     let attribute_value = AttributeValue::get_by_id(&ctx, &request.attribute_value_id)
    //         .await?
    //         .ok_or_else(|| {
    //             AttributeValueError::NotFound(request.attribute_value_id, *ctx.visibility())
    //         })?;
    //     let component = Component::get_by_id(&ctx, &attribute_value.context.component_id())
    //         .await?
    //         .ok_or_else(|| ComponentError::NotFound(attribute_value.context.component_id()))?;
    //
    //     let component_schema = component
    //         .schema(&ctx)
    //         .await?
    //         .ok_or(ComponentError::SchemaNotFound)?;
    //
    //     let prop = Prop::get_by_id(&ctx, &attribute_value.context.prop_id())
    //         .await?
    //         .ok_or(ComponentError::PropNotFound(
    //             attribute_value.context.prop_id(),
    //         ))?;
    //
    //     track(
    //         &posthog_client,
    //         &ctx,
    //         &original_uri,
    //         "default_function_restored",
    //         serde_json::json!({
    //             "component_id": component.id(),
    //             "component_schema_name": component_schema.name(),
    //             "prop_id": prop.id(),
    //             "prop_name": prop.name(),
    //         }),
    //     );
    // }

    ctx.commit().await?;

    let mut response = axum::response::Response::builder();
    if let Some(force_change_set_id) = force_change_set_id {
        response = response.header("force_change_set_id", force_change_set_id.to_string());
    }
    Ok(response.body(axum::body::Empty::new())?)
}
