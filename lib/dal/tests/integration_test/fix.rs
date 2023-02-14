use dal::action_prototype::ActionKind;
use dal::{
    workflow_runner::workflow_runner_state::WorkflowRunnerStatus, ActionPrototype,
    ActionPrototypeContext, AttributeReadContext, AttributeValue, AttributeValueId, ChangeSet,
    ChangeSetStatus, Component, DalContext, Fix, FixBatch, FixCompletionStatus, Func, FuncArgument,
    LeafInput, LeafInputLocation, LeafKind, RootPropChild, SchemaVariant, StandardModel,
    Visibility, WorkflowPrototype, WorkflowPrototypeId, WorkflowRunner,
};
use dal_test::helpers::component_payload::ComponentPayload;
use dal_test::{
    helpers::builtins::{Builtin, SchemaBuiltinsTestHarness},
    test,
};

// TODO(nick,paulo,paul,wendy): fix this test module once we are able to do so.
#[test]
#[ignore]
async fn confirmation_to_action(ctx: &mut DalContext) {
    let (payload, _attribute_value_id, action_workflow_prototype_id, _action_name) =
        setup_confirmation_resolver_and_get_action_prototype(ctx).await;

    // Apply the change set.
    let mut change_set = ChangeSet::get_by_pk(ctx, &ctx.visibility().change_set_pk)
        .await
        .expect("could not perform get by pk")
        .expect("could not get change set");
    change_set
        .apply(ctx)
        .await
        .expect("cannot apply change set");
    assert_eq!(&change_set.status, &ChangeSetStatus::Applied);
    ctx.update_visibility(Visibility::new_head(false));

    let run_id = rand::random();
    let (_runner, runner_state, func_binding_return_values, _resources) = WorkflowRunner::run(
        ctx,
        run_id,
        action_workflow_prototype_id,
        payload.component_id,
        true,
    )
    .await
    .expect("could not perform workflow runner run");
    assert_eq!(runner_state.status(), WorkflowRunnerStatus::Success);

    let mut maybe_skopeo_output_name = None;
    for func_binding_return_value in &func_binding_return_values {
        maybe_skopeo_output_name = maybe_skopeo_output_name.or_else(|| {
            func_binding_return_value
                .value()
                .and_then(|v| v.pointer("/value/Name"))
                .and_then(|v| v.as_str())
        });
    }
    assert_eq!(
        maybe_skopeo_output_name,
        Some("docker.io/systeminit/whiskers")
    );
}

// TODO(nick,paulo,paul,wendy): fix this test module once we are able to do so.
#[test]
#[ignore]
async fn confirmation_to_fix(ctx: &mut DalContext) {
    let (payload, attribute_value_id, action_workflow_prototype_id, action_name) =
        setup_confirmation_resolver_and_get_action_prototype(ctx).await;

    // Apply the change set.
    let mut change_set = ChangeSet::get_by_pk(ctx, &ctx.visibility().change_set_pk)
        .await
        .expect("could not perform get by pk")
        .expect("could not get change set");
    change_set
        .apply(ctx)
        .await
        .expect("cannot apply change set");
    assert_eq!(&change_set.status, &ChangeSetStatus::Applied);
    ctx.update_visibility(Visibility::new_head(false));

    // Create the batch.
    let mut batch = FixBatch::new(ctx, "toddhoward@systeminit.com")
        .await
        .expect("could not create fix execution batch");
    assert!(batch.started_at().is_none());
    assert!(batch.finished_at().is_none());
    assert!(batch.completion_status().is_none());

    // Create all fix(es) before starting the batch.
    let mut fix = Fix::new(
        ctx,
        *batch.id(),
        attribute_value_id,
        payload.component_id,
        &action_name,
    )
    .await
    .expect("could not create fix");
    assert!(fix.started_at().is_none());
    assert!(fix.finished_at().is_none());
    assert!(fix.completion_status().is_none());

    // NOTE(nick): batches are stamped as started inside their job.
    batch
        .stamp_started(ctx)
        .await
        .expect("could not stamp batch as started");
    assert!(batch.started_at().is_some());
    assert!(batch.finished_at().is_none());
    assert!(batch.completion_status().is_none());

    let run_id = rand::random();
    fix.run(ctx, run_id, action_workflow_prototype_id, true)
        .await
        .expect("could not run fix");
    assert!(fix.started_at().is_some());
    assert!(fix.finished_at().is_some());
    let completion_status = fix
        .completion_status()
        .expect("no completion status found for fix");
    assert_eq!(completion_status, &FixCompletionStatus::Success);

    // NOTE(nick): batches are stamped as finished inside their job.
    let batch_completion_status = batch
        .stamp_finished(ctx)
        .await
        .expect("could not complete batch");
    assert!(batch.finished_at().is_some());
    assert_eq!(
        batch
            .completion_status()
            .expect("no completion status for batch"),
        &FixCompletionStatus::Success
    );
    assert_eq!(batch_completion_status, FixCompletionStatus::Success);

    let found_batch = fix
        .fix_batch(ctx)
        .await
        .expect("could not get fix execution batch")
        .expect("no fix execution batch found");
    assert_eq!(batch.id(), found_batch.id());
}

async fn setup_confirmation_resolver_and_get_action_prototype(
    ctx: &DalContext,
) -> (
    ComponentPayload,
    AttributeValueId,
    WorkflowPrototypeId,
    String,
) {
    let mut harness = SchemaBuiltinsTestHarness::new();
    let payload = harness
        .create_component(ctx, "systeminit/whiskers", Builtin::DockerImage)
        .await;

    let confirmation_func_name = "si:confirmationResourceExists";
    let func = Func::find_by_attr(ctx, "name", &confirmation_func_name)
        .await
        .expect("unable to find func")
        .pop()
        .expect("func not found");
    let func_argument = FuncArgument::find_by_name_for_func(ctx, "resource", *func.id())
        .await
        .expect("could not perform find by name for func")
        .expect("func argument not found");

    // FIXME(nick): this is a misuse of how docker image works. We should not be using a builtin for
    // these tests.
    SchemaVariant::add_leaf(
        ctx,
        *func.id(),
        payload.schema_variant_id,
        Some(payload.component_id),
        LeafKind::Confirmation,
        vec![LeafInput {
            location: LeafInputLocation::Resource,
            func_argument_id: *func_argument.id(),
        }],
    )
    .await
    .expect("could not add leaf");

    let title = "Refresh Docker Image";
    let workflow_prototype = WorkflowPrototype::find_by_attr(ctx, "title", &title)
        .await
        .expect("unable to find prototype")
        .pop()
        .expect("unable to find prototype");

    let name = "create";
    let context = ActionPrototypeContext {
        schema_id: payload.schema_id,
        schema_variant_id: payload.schema_variant_id,
        ..Default::default()
    };
    ActionPrototype::new(
        ctx,
        *workflow_prototype.id(),
        name,
        ActionKind::Other,
        context,
    )
    .await
    .expect("unable to create action prototype");

    // TODO(nick): don't use builtins and actually apply the changeset.
    // Now, run confirmations as if the changeset was applied.
    Component::run_all_confirmations(ctx)
        .await
        .expect("could not run confirmations");

    let confirmation_map_attribute_value =
        Component::root_prop_child_attribute_value_for_component(
            ctx,
            payload.component_id,
            RootPropChild::Confirmation,
        )
        .await
        .expect("could not find root child attribute value for component");

    let confirmation_item_prop =
        SchemaVariant::find_leaf_item_prop(ctx, payload.schema_variant_id, LeafKind::Confirmation)
            .await
            .expect("could not find confirmation leaf item prop");
    let confirmation_attribute_value = AttributeValue::find_with_parent_and_key_for_context(
        ctx,
        Some(*confirmation_map_attribute_value.id()),
        Some(confirmation_func_name.to_string()),
        AttributeReadContext {
            prop_id: Some(*confirmation_item_prop.id()),
            component_id: Some(payload.component_id),
            ..AttributeReadContext::default()
        },
    )
    .await
    .expect("could not perform find with parent and key for context")
    .expect("could not find confirmation attribute value");

    let mut found_confirmations = Component::list_confirmations(ctx)
        .await
        .expect("could not list confirmations");
    let found_confirmation = found_confirmations
        .pop()
        .expect("found confirmations are empty");
    assert!(found_confirmations.is_empty());
    assert_eq!(
        found_confirmation.attribute_value_id,
        *confirmation_attribute_value.id()
    );

    // FIXME(nick): re-write this test. Something is wrong here.
    let action_prototype =
        ActionPrototype::find_by_name(ctx, "create", payload.schema_id, payload.schema_variant_id)
            .await
            .expect("could not find action prototype")
            .expect("no action prototype found");

    (
        payload,
        found_confirmation.attribute_value_id,
        action_prototype.workflow_prototype_id(),
        action_prototype.name().to_string(),
    )
}
