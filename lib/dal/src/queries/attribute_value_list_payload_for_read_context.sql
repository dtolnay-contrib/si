SELECT DISTINCT ON (
    attribute_values.attribute_context_prop_id,
    attribute_value_belongs_to_attribute_value.belongs_to_id,
    attribute_values.key
)
    attribute_values.id,
    attribute_values.visibility_change_set_pk,
    attribute_values.visibility_edit_session_pk,
    attribute_values.attribute_context_prop_id,
    attribute_values.attribute_context_internal_provider_id,
    attribute_values.attribute_context_external_provider_id,
    attribute_values.attribute_context_schema_id,
    attribute_values.attribute_context_schema_variant_id,
    attribute_values.attribute_context_component_id,
    attribute_values.attribute_context_system_id,
    parent_attribute_values.id AS parent_attribute_value_id,
    row_to_json(attribute_values.*) AS attribute_value_object,
    row_to_json(props.*) AS prop_object,
    row_to_json(func_binding_return_values) AS object
FROM attribute_values
INNER JOIN props ON
    props.id = attribute_values.attribute_context_prop_id
INNER JOIN func_binding_return_values ON
    func_binding_return_values.id = attribute_values.func_binding_return_value_id
    AND is_visible_v1($2, func_binding_return_values.visibility_change_set_pk, func_binding_return_values.visibility_edit_session_pk,
                      func_binding_return_values.visibility_deleted_at)
LEFT JOIN attribute_value_belongs_to_attribute_value ON
    attribute_values.id = attribute_value_belongs_to_attribute_value.object_id
    AND is_visible_v1($2, attribute_value_belongs_to_attribute_value.visibility_change_set_pk, attribute_value_belongs_to_attribute_value.visibility_edit_session_pk,
                      attribute_value_belongs_to_attribute_value.visibility_deleted_at)
LEFT JOIN attribute_values AS parent_attribute_values ON
    attribute_value_belongs_to_attribute_value.belongs_to_id = parent_attribute_values.id
    AND is_visible_v1($2, parent_attribute_values.visibility_change_set_pk, parent_attribute_values.visibility_edit_session_pk,
                      parent_attribute_values.visibility_deleted_at)
WHERE in_tenancy_v1($1, attribute_values.tenancy_universal, attribute_values.tenancy_billing_account_ids, attribute_values.tenancy_organization_ids,
                    attribute_values.tenancy_workspace_ids)
    AND is_visible_v1($2, attribute_values.visibility_change_set_pk, attribute_values.visibility_edit_session_pk, attribute_values.visibility_deleted_at)
    AND in_attribute_context_v1($3, attribute_values.attribute_context_prop_id,
                                    attribute_values.attribute_context_internal_provider_id,
                                    attribute_values.attribute_context_external_provider_id,
                                    attribute_values.attribute_context_schema_id,
                                    attribute_values.attribute_context_schema_variant_id,
                                    attribute_values.attribute_context_component_id,
                                    attribute_values.attribute_context_system_id)
    AND attribute_values.attribute_context_prop_id IN (
      SELECT DISTINCT ON (props.id) props.id
        FROM props
        LEFT JOIN (
          SELECT 
            prop_belongs_to_prop.belongs_to_id AS belongs_to_id, 
            array_agg(prop_belongs_to_prop.object_id) AS child_prop_ids 
            FROM prop_belongs_to_prop 
            GROUP BY prop_belongs_to_prop.belongs_to_id
          ) AS child_prop_ids ON child_prop_ids.belongs_to_id = props.id
        WHERE in_tenancy_v1(
            $1, 
            props.tenancy_universal, 
            props.tenancy_billing_account_ids, 
            props.tenancy_organization_ids,
            props.tenancy_workspace_ids
          )
          AND is_visible_v1(
            $2, 
            props.visibility_change_set_pk, 
            props.visibility_edit_session_pk, 
            props.visibility_deleted_at
          )
          AND props.id IN (
            WITH RECURSIVE recursive_props AS (
                SELECT left_object_id AS prop_id
                  FROM prop_many_to_many_schema_variants
                  WHERE right_object_id = $4
                UNION ALL
                  SELECT pbp.object_id AS prop_id
                  FROM prop_belongs_to_prop AS pbp
                  JOIN recursive_props ON pbp.belongs_to_id = recursive_props.prop_id
              )
              SELECT prop_id
                FROM recursive_props
          )
      ORDER BY props.id
    )
ORDER BY
    attribute_values.attribute_context_prop_id,
    attribute_value_belongs_to_attribute_value.belongs_to_id,
    attribute_values.key,
    visibility_change_set_pk DESC,
    visibility_edit_session_pk DESC,
    attribute_context_internal_provider_id DESC,
    attribute_context_external_provider_id DESC,
    attribute_context_schema_id DESC,
    attribute_context_schema_variant_id DESC,
    attribute_context_component_id DESC,
    attribute_context_system_id DESC;
