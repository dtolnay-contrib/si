use axum::extract::Query;
use axum::Json;
use dal::{
    Component, ComponentId, LabelEntry, LabelList, StandardModel, Tenancy, Visibility, Workspace,
    WorkspaceId,
};
use serde::{Deserialize, Serialize};

use super::{ComponentError, ComponentResult};
use crate::server::extract::{Authorization, PgRoTxn};

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ListComponentNamesOnlyRequest {
    pub workspace_id: WorkspaceId,
    #[serde(flatten)]
    pub visibility: Visibility,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ListComponentNamesOnlyItem {
    pub id: ComponentId,
    pub name: String,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ListComponentNamesOnlyResponse {
    pub list: LabelList<ComponentId>,
}

// NOTE(nick): this name is long and cumbersome, but the hole has been dug for this dummy data
// provider. Future changes to this code should consider renaming this (and its route, TS client,
// etc.) to something more readable, such as "list_component_names".
pub async fn list_components_names_only(
    mut txn: PgRoTxn,
    Query(request): Query<ListComponentNamesOnlyRequest>,
    Authorization(claim): Authorization,
) -> ComponentResult<Json<ListComponentNamesOnlyResponse>> {
    let txn = txn.start().await?;
    let billing_account_tenancy = Tenancy::new_billing_account(vec![claim.billing_account_id]);
    let workspace = Workspace::get_by_id(
        &txn,
        &billing_account_tenancy,
        &request.visibility,
        &request.workspace_id,
    )
    .await?
    .ok_or(ComponentError::InvalidRequest)?;
    let tenancy = Tenancy::new_workspace(vec![*workspace.id()]);

    let components = Component::list(&txn, &tenancy, &request.visibility).await?;
    let label_entries: Vec<LabelEntry<ComponentId>> = components
        .into_iter()
        .map(|component| LabelEntry {
            label: component.name().to_string(),
            value: *component.id(),
        })
        .collect();
    let list = LabelList::from(label_entries);
    let response = ListComponentNamesOnlyResponse { list };
    Ok(Json(response))
}
