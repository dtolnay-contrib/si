use serde::{Deserialize, Serialize};
use si_data::{NatsError, NatsTxn, PgError, PgTxn};
use telemetry::prelude::*;
use thiserror::Error;

use crate::{
    impl_standard_model, pk, standard_model, standard_model_accessor, standard_model_belongs_to,
    standard_model_has_many, HistoryActor, HistoryEventError, Organization, OrganizationId,
    StandardModel, StandardModelError, System, SystemResult, Tenancy, Timestamp, Visibility,
    WriteTenancy,
};

#[derive(Error, Debug)]
pub enum WorkspaceError {
    #[error("error serializing/deserializing json: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("pg error: {0}")]
    Pg(#[from] PgError),
    #[error("nats txn error: {0}")]
    Nats(#[from] NatsError),
    #[error("history event error: {0}")]
    HistoryEvent(#[from] HistoryEventError),
    #[error("standard model error: {0}")]
    StandardModelError(#[from] StandardModelError),
}

pub type WorkspaceResult<T> = Result<T, WorkspaceError>;

pk!(WorkspacePk);
pk!(WorkspaceId);

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pk: WorkspacePk,
    id: WorkspaceId,
    name: String,
    #[serde(flatten)]
    tenancy: Tenancy,
    #[serde(flatten)]
    timestamp: Timestamp,
    #[serde(flatten)]
    visibility: Visibility,
}

impl_standard_model! {
    model: Workspace,
    pk: WorkspacePk,
    id: WorkspaceId,
    table_name: "workspaces",
    history_event_label_base: "workspace",
    history_event_message_name: "Workspace"
}

impl Workspace {
    #[instrument(skip_all)]
    pub async fn new(
        txn: &PgTxn<'_>,
        nats: &NatsTxn,
        write_tenancy: &WriteTenancy,
        visibility: &Visibility,
        history_actor: &HistoryActor,
        name: impl AsRef<str>,
    ) -> WorkspaceResult<Self> {
        let name = name.as_ref();
        let row = txn
            .query_one(
                "SELECT object FROM workspace_create_v1($1, $2, $3)",
                &[write_tenancy, &visibility, &name],
            )
            .await?;
        let object = standard_model::finish_create_from_row(
            txn,
            nats,
            &write_tenancy.into(),
            visibility,
            history_actor,
            row,
        )
        .await?;
        Ok(object)
    }

    standard_model_accessor!(name, String, WorkspaceResult);

    standard_model_belongs_to!(
        lookup_fn: organization,
        set_fn: set_organization,
        unset_fn: unset_organization,
        table: "workspace_belongs_to_organization",
        model_table: "organizations",
        belongs_to_id: OrganizationId,
        returns: Organization,
        result: WorkspaceResult,
    );

    standard_model_has_many!(
        lookup_fn: systems,
        table: "system_belongs_to_workspace",
        model_table: "systems",
        returns: System,
        result: SystemResult,
    );
}
