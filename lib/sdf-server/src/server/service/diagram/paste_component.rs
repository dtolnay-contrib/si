use axum::{extract::OriginalUri, http::uri::Uri};
use axum::{response::IntoResponse, Json};
use chrono::Utc;
use dal::edge::EdgeKind;
use dal::{
    action_prototype::ActionPrototypeContextField, func::backend::js_action::ActionRunResult,
    Action, ActionKind, ActionPrototype, ActionPrototypeContext, ChangeSet, Component,
    ComponentError, ComponentId, Connection, DalContext, DalContextBuilder, Edge, Node, NodeId,
    StandardModel, Visibility, WsEvent,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use telemetry::prelude::*;
use tokio::task::JoinSet;
use ulid::Ulid;
use veritech_client::ResourceStatus;

use super::{DiagramError, DiagramResult};
use crate::server::extract::{AccessBuilder, HandlerContext, PosthogClient};
use crate::server::service::async_route::handle_error;
use crate::server::tracking::track;
use crate::service::diagram::connect_component_to_frame::connect_component_sockets_to_frame;

#[allow(clippy::too_many_arguments)]
async fn paste_single_component(
    ctx_builder: DalContextBuilder,
    request_ctx: dal::context::AccessBuilder,
    visibility: Visibility,
    component_id: ComponentId,
    offset_x: f64,
    offset_y: f64,
    original_uri: &Uri,
    PosthogClient(posthog_client): &PosthogClient,
) -> DiagramResult<Component> {
    let ctx = ctx_builder.build(request_ctx.build(visibility)).await?;

    let original_comp = Component::get_by_id(&ctx, &component_id)
        .await?
        .ok_or(DiagramError::ComponentNotFound)?;

    let schema_variant = original_comp
        .schema_variant(&ctx)
        .await?
        .ok_or(DiagramError::SchemaNotFound)?;

    let mut pasted_comp =
        Component::new(&ctx, original_comp.name(&ctx).await?, *schema_variant.id()).await?;
    let x: f64 = original_comp.x().parse()?;
    let y: f64 = original_comp.y().parse()?;
    pasted_comp
        .set_geometry(
            &ctx,
            (x + offset_x).to_string(),
            (y + offset_y).to_string(),
            original_comp.width(),
            original_comp.height(),
        )
        .await?;
    ctx.commit().await?;

    pasted_comp
        .clone_attributes_from(&ctx, *original_comp.id())
        .await?;

    pasted_comp
        .set_resource_raw(
            &ctx,
            ActionRunResult {
                status: Some(ResourceStatus::Ok),
                payload: None,
                message: None,
                logs: Vec::new(),
                last_synced: Some(Utc::now().to_rfc3339()),
            },
            false,
        )
        .await?;

    pasted_comp
        .set_name(
            &ctx,
            Some(format!("{} - Copy", original_comp.name(&ctx).await?)),
        )
        .await?;

    ctx.commit().await?;

    for prototype in ActionPrototype::find_for_context_and_kind(
        &ctx,
        ActionKind::Create,
        ActionPrototypeContext::new_for_context_field(ActionPrototypeContextField::SchemaVariant(
            *schema_variant.id(),
        )),
    )
    .await?
    {
        let action = Action::upsert(&ctx, *prototype.id(), *pasted_comp.id()).await?;
        let prototype = action.prototype(&ctx).await?;

        track(
            posthog_client,
            &ctx,
            original_uri,
            "create_action",
            serde_json::json!({
                "how": "/diagram/paste_components",
                "prototype_id": prototype.id(),
                "prototype_kind": prototype.kind(),
                "component_id": pasted_comp.id(),
                "component_name": pasted_comp.name(&ctx).await?,
                "change_set_pk": ctx.visibility().change_set_pk,
            }),
        );
    }

    let schema = pasted_comp
        .schema(&ctx)
        .await?
        .ok_or(DiagramError::SchemaNotFound)?;
    track(
        posthog_client,
        &ctx,
        original_uri,
        "paste_component",
        serde_json::json!({
            "component_id": pasted_comp.id(),
            "component_schema_name": schema.name(),
        }),
    );

    WsEvent::component_created(&ctx, *pasted_comp.id())
        .await?
        .publish_on_commit(&ctx)
        .await?;

    ctx.commit().await?;

    Ok(pasted_comp)
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PasteComponentsRequest {
    pub component_ids: Vec<ComponentId>,
    pub offset_x: f64,
    pub offset_y: f64,
    pub new_parent_component_id: Option<ComponentId>,
    #[serde(flatten)]
    pub visibility: Visibility,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PasteComponentsResponse {
    pub id: Ulid,
}

/// Paste a set of [`Component`](dal::Component)s via their componentId. Creates change-set if on head
pub async fn paste_components(
    HandlerContext(builder): HandlerContext,
    AccessBuilder(request_ctx): AccessBuilder,
    PosthogClient(posthog_client): PosthogClient,
    OriginalUri(original_uri): OriginalUri,
    Json(request): Json<PasteComponentsRequest>,
) -> DiagramResult<impl IntoResponse> {
    let mut ctx = builder.build(request_ctx.build(request.visibility)).await?;

    let maybe_force_change_set_id = ChangeSet::force_new(&mut ctx).await?;
    ctx.commit().await?;

    let id = Ulid::new();
    tokio::task::spawn(async move {
        if let Err(err) = paste_components_inner(
            request_ctx,
            &ctx,
            request,
            &original_uri,
            PosthogClient(posthog_client),
        )
        .await
        {
            handle_error(&ctx, original_uri, id, err).await;
        } else {
            match WsEvent::async_finish(&ctx, id).await {
                Ok(event) => match event.publish_on_commit(&ctx).await {
                    Ok(()) => {
                        if let Err(err) = ctx.commit().await {
                            handle_error(&ctx, original_uri, id, err).await;
                        }
                    }
                    Err(err) => handle_error(&ctx, original_uri, id, err).await,
                },
                Err(err) => {
                    handle_error(&ctx, original_uri, id, err).await;
                }
            }
        }
    });

    let mut response = axum::response::Response::builder();
    if let Some(force_change_set_id) = maybe_force_change_set_id {
        response = response.header("force_change_set_id", force_change_set_id.to_string());
    }

    Ok(response.body(serde_json::to_string(&PasteComponentsResponse { id })?)?)
}

async fn paste_components_inner(
    request_ctx: dal::AccessBuilder,
    ctx: &DalContext,
    request: PasteComponentsRequest,
    original_uri: &Uri,
    PosthogClient(posthog_client): PosthogClient,
) -> DiagramResult<()> {
    let mut tasks = JoinSet::new();

    let mut pasted_components_by_original = HashMap::new();
    for component_id in &request.component_ids {
        let ctx_builder = ctx.to_builder();
        let (visibility, component_id) = (*ctx.visibility(), *component_id);
        let (offset_x, offset_y) = (request.offset_x, request.offset_y);
        let (original_uri, posthog_client) =
            (original_uri.clone(), PosthogClient(posthog_client.clone()));
        tasks.spawn(async move {
            let pasted_comp = paste_single_component(
                ctx_builder,
                request_ctx,
                visibility,
                component_id,
                offset_x,
                offset_y,
                &original_uri,
                &posthog_client,
            )
            .await?;

            Ok::<_, DiagramError>((component_id, pasted_comp))
        });
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok((component_id, pasted_comp))) => {
                pasted_components_by_original.insert(component_id, pasted_comp);
            }
            Ok(Err(err)) => return Err(err)?,
            // Task panicked, let's propagate it
            Err(err) => match err.try_into_panic() {
                Ok(panic) => {
                    std::panic::resume_unwind(panic);
                }
                Err(err) => {
                    if err.is_cancelled() {
                        warn!("Paste Component was cancelled: {err}");
                    } else {
                        error!("Unknown failure in component paste: {err}");
                    }
                }
            },
        }
    }

    for component_id in &request.component_ids {
        let pasted_component =
            if let Some(component) = pasted_components_by_original.get(component_id) {
                component
            } else {
                return Err(DiagramError::PasteError);
            };

        let edges = Edge::list_for_component(ctx, *component_id)
            .await?
            .into_iter();

        let mut has_parent = false;

        // Copy edges if peer is on set
        for edge in edges {
            if let (Some(tail_component), Some(head_component)) = (
                pasted_components_by_original.get(&edge.tail_component_id()),
                pasted_components_by_original.get(&edge.head_component_id()),
            ) {
                if *edge.kind() == EdgeKind::Symbolic && edge.tail_component_id() == *component_id {
                    has_parent = true;
                }

                Connection::new(
                    ctx,
                    *tail_component.id(),
                    edge.tail_socket_id(),
                    *head_component.id(),
                    edge.head_socket_id(),
                    *edge.kind(),
                )
                .await?;
            }
        }

        if let Some(parent_id) = request.new_parent_component_id {
            if !has_parent {
                connect_component_sockets_to_frame(
                    ctx,
                    parent_id,
                    *pasted_component.id(),
                    original_uri,
                    &posthog_client,
                )
                .await?;
            }
        }
    }

    Ok(())
}
