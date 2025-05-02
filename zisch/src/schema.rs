// @generated automatically by Diesel CLI.

diesel::table! {
    build_configs (id) {
        id -> Integer,
        name -> Text,
    }
}

diesel::table! {
    files (id) {
        id -> Integer,
        build_config_id -> Nullable<Integer>,
        rel_path -> Text,
        content_hash -> Binary,
    }
}

diesel::joinable!(files -> build_configs (build_config_id));

diesel::allow_tables_to_appear_in_same_query!(
    build_configs,
    files,
);
