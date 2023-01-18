//! Expansion implementation of the `dal_test` attribute macro.
//!
//! This implementation is a combination of a configurable threaded Tokio runtime (formerly
//! provided via the `tokio::test` macro from the `tokio` crate), support for optional
//! tracing/logging support (formerly provided via the `test` mecro from the `test-env-log` crate),
//! and an "extractor"-style dependency setup a little like axum's extractors.
//!
//! # Reference Implementations and Credits
//!
//! * [`tokio::test` macro](https://github.com/tokio-rs/tokio/blob/121769c762ad6b1686ecd0e8618005aab8b7e980/tokio-macros/src/entry.rs)
//! * [`test_env_log::test` macro](https://github.com/d-e-s-o/test-log/blob/544dbac50321aaf580959ad7a7997358517db198/src/lib.rs)

use std::sync::Arc;

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{
    parse_quote, punctuated::Punctuated, token::Comma, AttributeArgs, Expr, FnArg, ItemFn, Path,
    ReturnType, Type,
};

const LOG_ENV_VAR: &str = "SI_TEST_LOG";
const SPAN_EVENTS_ENV_VAR: &str = "SI_TEST_LOG_SPAN_EVENTS";

const RT_DEFAULT_WORKER_THREADS: usize = 2;
const RT_DEFAULT_THREAD_STACK_SIZE: usize = 2 * 1024 * 1024 * 3;

pub(crate) fn expand(item: ItemFn, _args: AttributeArgs) -> TokenStream {
    if item.sig.asyncness.is_none() {
        panic!("test function must be async--blocking tests not supported");
    }

    let attrs = &item.attrs;
    let body = &item.block;
    let test_name = &item.sig.ident;
    let params = &item.sig.inputs;
    // Note that Rust doesn't allow a test function with `#[should_panic]` that has a non-unit
    // return value. Huh
    let (rt_is_result, output) = match &item.sig.output {
        ReturnType::Default => (false, quote! {}),
        ReturnType::Type(_, typeness) => (true, quote! {-> #typeness}),
    };
    let test_attr = quote! {#[::core::prelude::v1::test]};

    let worker_threads = RT_DEFAULT_WORKER_THREADS;
    let thread_stack_size = RT_DEFAULT_THREAD_STACK_SIZE;

    let fn_setup = fn_setup(item.sig.inputs.iter());
    let fn_setups = fn_setup.code;
    let fn_args = fn_setup.fn_args;
    let fn_call = if rt_is_result {
        quote! {let _ = test_fn(#fn_args).await?;}
    } else {
        quote! {test_fn(#fn_args).await;}
    };
    let color_eyre_init = expand_color_eyre_init();
    let tracing_init = expand_tracing_init();
    let rt = expand_runtime(worker_threads, thread_stack_size);

    quote! {
        #test_attr
        #(#attrs)*
        fn #test_name() -> ::dal_test::Result<()> {
            use ::dal_test::WrapErr;

            async fn test_fn(#params) #output #body

            #[inline]
            async fn spawned_task() -> ::dal_test::Result<()> {
                #fn_setups
                #fn_call
                Ok(())
            }

            ::dal_test::COLOR_EYRE_INIT.call_once(|| {
                #color_eyre_init
                #tracing_init
            });

            let thread_builder = ::std::thread::Builder::new().stack_size(#thread_stack_size);
            let thread_join_handle = thread_builder.spawn(|| {
                #[allow(clippy::expect_used)]
                #rt.block_on(spawned_task())
            }).expect("failed to spawn thread at OS level");
            let test_result = match thread_join_handle.join() {
                Ok(r) => r,
                Err(err) => {
                    // Spawned test task panicked
                    ::std::panic::resume_unwind(err);
                }
            };
            let _ = test_result?;

            Ok(())
        }
    }
}

fn fn_setup<'a>(params: impl Iterator<Item = &'a FnArg>) -> FnSetup {
    let mut expander = FnSetupExpander::new();

    for param in params {
        match param {
            FnArg::Typed(pat_type) => match &*pat_type.ty {
                Type::Path(type_path) => {
                    let path = path_as_string(&type_path.path);
                    if let Some(ty_str) = path.split("::").last() {
                        // Each string match corresponds to an imported type that corresponds to an
                        // **owned** variable. For example:
                        //
                        // ```ignore
                        // #[test]
                        // async fn does_things(bid: BillingAccountId) {
                        //      // ...
                        // }
                        // ```
                        //
                        // Note that several types such as `DalContextHead` may have interior
                        // references and/or mutability, however the surrounding type is passed as
                        // an owned type.
                        match ty_str {
                            "BillingAccountId" => {
                                let var = expander.setup_billing_account_id();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "BillingAccountSignup" => {
                                let var = expander.setup_billing_account_signup();
                                let var = var.0.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "Connections" => {
                                let var = expander.setup_owned_connections();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "DalContext" => {
                                let var = expander.setup_dal_context_default();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "DalContextBuilder" => {
                                let var = expander.setup_dal_context_builder();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "DalContextHead" => {
                                let var = expander.setup_dal_context_head();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "DalContextHeadRef" => {
                                let var = expander.setup_dal_context_head_ref();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "DalContextHeadMutRef" => {
                                let var = expander.setup_dal_context_head_mut_ref();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "DalContextUniversalHead" => {
                                let var = expander.setup_dal_context_universal_head();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "DalContextUniversalHeadRef" => {
                                let var = expander.setup_dal_context_universal_head_ref();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "DalContextUniversalHeadMutRef" => {
                                let var = expander.setup_dal_context_universal_head_mut_ref();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "OrganizationId" => {
                                let var = expander.setup_organization_id();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "ServicesContext" => {
                                let var = expander.setup_services_context();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "ShutdownHandle" => {
                                let var = expander.setup_veritech_shutdown_handle();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            "WorkspaceId" => {
                                let var = expander.setup_workspace_id();
                                let var = var.as_ref();
                                expander.push_arg(parse_quote! {#var});
                            }
                            _ => panic!("unexpected argument type: {:?}", type_path),
                        };
                    }
                }
                Type::Reference(type_ref) => match &*type_ref.elem {
                    Type::Path(type_path) => {
                        let path = path_as_string(&type_path.path);
                        if let Some(ty_str) = path.split("::").last() {
                            // Each string match corresponds to an imported type that corresponds
                            // to an **borrowed**/**referenced** variable. For example:
                            //
                            // ```ignore
                            // #[test]
                            // async fn does_things(
                            //      ctx: &mut DalContext,
                            //      nba: &BillingAccountSignup
                            //  ) {
                            //      // ...
                            // }
                            // ```
                            //
                            // In the above example, both would be matched types in this section,
                            // even though `ctx` is a mutable reference and `nba` is an immutable
                            // reference.
                            match ty_str {
                                "BillingAccountSignup" => {
                                    let var = expander.setup_billing_account_signup();
                                    let var = var.0.as_ref();
                                    expander.push_arg(parse_quote! {&#var});
                                }
                                "DalContext" => {
                                    if type_ref.mutability.is_some() {
                                        let var = expander.setup_dal_context_default_mut();
                                        let var = var.as_ref();
                                        expander.push_arg(parse_quote! {&mut #var});
                                    } else {
                                        let var = expander.setup_dal_context_default();
                                        let var = var.as_ref();
                                        expander.push_arg(parse_quote! {&#var});
                                    }
                                }
                                "DalContextBuilder" => {
                                    let var = expander.setup_dal_context_builder();
                                    let var = var.as_ref();
                                    expander.push_arg(parse_quote! {&#var});
                                }
                                "JwtSecretKey" => {
                                    let var = expander.setup_jwt_secret_key();
                                    let var = var.as_ref();
                                    expander.push_arg(parse_quote! {#var});
                                }
                                "ServicesContext" => {
                                    let var = expander.setup_services_context();
                                    let var = var.as_ref();
                                    expander.push_arg(parse_quote! {&#var});
                                }
                                _ => panic!("unexpected argument reference type: {:?}", type_ref),
                            }
                        }
                    }
                    unsupported => {
                        panic!("argument reference type not supported: {:?}", unsupported)
                    }
                },
                unsupported => panic!("argument type not supported: {:?}", unsupported),
            },
            FnArg::Receiver(_) => {
                panic!("argument does not support receiver/method style (i.e. using `self`)")
            }
        }
    }

    if expander.has_args() {
        // TODO(fnichol): we can use a macro attribute to opt-out and not run a veritech server in the
        // future, but for now (as before), every test starts with its own veritech server with a
        // randomized subject prefix
        expander.setup_start_veritech_server();
        expander.setup_start_council_server();
    }

    expander.drop_transactions_clone_if_created();
    expander.finish()
}

struct FnSetup {
    code: TokenStream,
    fn_args: Punctuated<Expr, Comma>,
}

struct FnSetupExpander {
    code: TokenStream,
    args: Punctuated<Expr, Comma>,

    test_context: Option<Arc<Ident>>,
    jwt_secret_key: Option<Arc<Ident>>,
    nats_subject_prefix: Option<Arc<Ident>>,
    council_server: Option<Arc<Ident>>,
    start_council_server: Option<()>,
    veritech_server: Option<Arc<Ident>>,
    veritech_shutdown_handle: Option<Arc<Ident>>,
    start_veritech_server: Option<()>,
    services_context: Option<Arc<Ident>>,
    dal_context_builder: Option<Arc<Ident>>,
    connections: Option<Arc<Ident>>,
    owned_connections: Option<Arc<Ident>>,
    transactions: Option<Arc<Ident>>,
    billing_account_signup: Option<(Arc<Ident>, Arc<Ident>)>,
    billing_account_id: Option<Arc<Ident>>,
    organization_id: Option<Arc<Ident>>,
    workspace_id: Option<Arc<Ident>>,
    dal_context_default: Option<Arc<Ident>>,
    dal_context_default_mut: Option<Arc<Ident>>,
    dal_context_head: Option<Arc<Ident>>,
    dal_context_head_ref: Option<Arc<Ident>>,
    dal_context_head_mut_ref: Option<Arc<Ident>>,
    dal_context_universal_head: Option<Arc<Ident>>,
    dal_context_universal_head_ref: Option<Arc<Ident>>,
    dal_context_universal_head_mut_ref: Option<Arc<Ident>>,
}

impl FnSetupExpander {
    fn new() -> Self {
        Self {
            code: TokenStream::new(),
            args: Punctuated::new(),
            test_context: None,
            jwt_secret_key: None,
            nats_subject_prefix: None,
            council_server: None,
            start_council_server: None,
            veritech_server: None,
            veritech_shutdown_handle: None,
            start_veritech_server: None,
            services_context: None,
            dal_context_builder: None,
            connections: None,
            owned_connections: None,
            transactions: None,
            billing_account_signup: None,
            billing_account_id: None,
            organization_id: None,
            workspace_id: None,
            dal_context_default: None,
            dal_context_default_mut: None,
            dal_context_head: None,
            dal_context_head_ref: None,
            dal_context_head_mut_ref: None,
            dal_context_universal_head: None,
            dal_context_universal_head_ref: None,
            dal_context_universal_head_mut_ref: None,
        }
    }

    fn push_arg(&mut self, arg: Expr) {
        self.args.push(arg);
    }

    fn setup_test_context(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.test_context {
            return ident.clone();
        }

        let var = Ident::new("test_context", Span::call_site());
        self.code.extend(quote! {
            let test_context = ::dal_test::TestContext::global().await?;
        });
        self.test_context = Some(Arc::new(var));

        self.test_context.as_ref().unwrap().clone()
    }

    fn setup_jwt_secret_key(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.jwt_secret_key {
            return ident.clone();
        }

        let test_context = self.setup_test_context();
        let test_context = test_context.as_ref();

        let var = Ident::new("jwt_secret_key", Span::call_site());
        self.code.extend(quote! {
            let #var = #test_context.jwt_secret_key();
        });
        self.jwt_secret_key = Some(Arc::new(var));

        self.jwt_secret_key.as_ref().unwrap().clone()
    }

    fn setup_nats_subject_prefix(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.nats_subject_prefix {
            return ident.clone();
        }

        let var = Ident::new("nats_subject_prefix", Span::call_site());
        self.code.extend(quote! {
            let #var = ::dal_test::nats_subject_prefix();
        });
        self.nats_subject_prefix = Some(Arc::new(var));

        self.nats_subject_prefix.as_ref().unwrap().clone()
    }

    fn setup_council_server(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.council_server {
            return ident.clone();
        }

        let test_context = self.setup_test_context();
        let test_context = test_context.as_ref();
        let nats_subject_prefix = self.setup_nats_subject_prefix();
        let nats_subject_prefix = nats_subject_prefix.as_ref();

        let var = Ident::new("council_server", Span::call_site());
        self.code.extend(quote! {
            let #var = ::dal_test::council_server(
                #test_context.nats_config().clone(),
                format!("{}.council", #nats_subject_prefix),
            ).await?;
        });
        self.council_server = Some(Arc::new(var));

        self.council_server.as_ref().unwrap().clone()
    }

    fn setup_start_council_server(&mut self) {
        if self.start_council_server.is_some() {
            return;
        }

        let council_server = self.setup_council_server();
        let council_server = council_server.as_ref();

        self.code.extend(quote! {
            {
              let (_, shutdown_request_rx) = ::tokio::sync::watch::channel(());
              let (subscription_started_tx, mut subscription_started_rx) = ::tokio::sync::watch::channel(());
              ::tokio::spawn(#council_server.run(subscription_started_tx, shutdown_request_rx));
              subscription_started_rx.changed().await.unwrap()
            }
        });
        self.start_council_server = Some(());
    }

    fn setup_veritech_server(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.veritech_server {
            return ident.clone();
        }

        let test_context = self.setup_test_context();
        let test_context = test_context.as_ref();
        let nats_subject_prefix = self.setup_nats_subject_prefix();
        let nats_subject_prefix = nats_subject_prefix.as_ref();

        let var = Ident::new("veritech_server", Span::call_site());
        self.code.extend(quote! {
            let #var = ::dal_test::veritech_server_for_uds_cyclone(
                #test_context.nats_config().clone(),
                format!("{}.veritech", #nats_subject_prefix),
            ).await?;
        });
        self.veritech_server = Some(Arc::new(var));

        self.veritech_server.as_ref().unwrap().clone()
    }

    fn setup_veritech_shutdown_handle(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.veritech_shutdown_handle {
            return ident.clone();
        }

        let veritech_server = self.setup_veritech_server();
        let veritech_server = veritech_server.as_ref();

        let var = Ident::new("veritech_shutdown_handle", Span::call_site());
        self.code.extend(quote! {
            let #var = #veritech_server.shutdown_handle();
        });
        self.veritech_shutdown_handle = Some(Arc::new(var));

        self.veritech_shutdown_handle.as_ref().unwrap().clone()
    }

    fn setup_start_veritech_server(&mut self) {
        if self.start_veritech_server.is_some() {
            return;
        }

        let veritech_server = self.setup_veritech_server();
        let veritech_server = veritech_server.as_ref();

        self.code.extend(quote! {
            ::tokio::spawn(#veritech_server.run());
        });
        self.start_veritech_server = Some(());
    }

    fn setup_services_context(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.services_context {
            return ident.clone();
        }

        let test_context = self.setup_test_context();
        let test_context = test_context.as_ref();
        let nats_subject_prefix = self.setup_nats_subject_prefix();
        let nats_subject_prefix = nats_subject_prefix.as_ref();

        let var = Ident::new("services_context", Span::call_site());
        self.code.extend(quote! {
            let #var = #test_context.create_services_context(&#nats_subject_prefix).await;
        });
        self.services_context = Some(Arc::new(var));

        self.services_context.as_ref().unwrap().clone()
    }

    fn setup_dal_context_builder(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.dal_context_builder {
            return ident.clone();
        }

        let services_context = self.setup_services_context();
        let services_context = services_context.as_ref();

        let var = Ident::new("dal_context_builder", Span::call_site());
        self.code.extend(quote! {
            let #var = #services_context.into_builder();
        });
        self.dal_context_builder = Some(Arc::new(var));

        self.dal_context_builder.as_ref().unwrap().clone()
    }

    fn setup_connections(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.connections {
            return ident.clone();
        }

        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();

        let var = Ident::new("connections", Span::call_site());
        self.code.extend(quote! {
            let mut #var = #dal_context_builder
                .connections()
                .await
                .wrap_err("failed to build connections")?;
        });
        self.connections = Some(Arc::new(var));

        self.connections.as_ref().unwrap().clone()
    }

    fn setup_owned_connections(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.owned_connections {
            return ident.clone();
        }

        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();

        let var = Ident::new("owned_connections", Span::call_site());
        self.code.extend(quote! {
            let #var = #dal_context_builder
                .connections()
                .await
                .wrap_err("failed to build connections")?;
        });
        self.owned_connections = Some(Arc::new(var));

        self.owned_connections.as_ref().unwrap().clone()
    }

    fn setup_transactions(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.transactions {
            return ident.clone();
        }

        let connections = self.setup_connections();
        let connections = connections.as_ref();

        let var = Ident::new("transactions", Span::call_site());
        self.code.extend(quote! {
            let mut #var = #connections
                .start_txns()
                .await
                .wrap_err("failed to start transactions")?;
        });
        self.transactions = Some(Arc::new(var));

        self.transactions.as_ref().unwrap().clone()
    }

    fn setup_billing_account_signup(&mut self) -> (Arc<Ident>, Arc<Ident>) {
        if let Some(ref idents) = self.billing_account_signup {
            return idents.clone();
        }

        let test_context = self.setup_test_context();
        let test_context = test_context.as_ref();
        let transactions = self.setup_transactions();
        let transactions = transactions.as_ref();
        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();

        let var_nba = Ident::new("nba", Span::call_site());
        let var_auth_token = Ident::new("auth_token", Span::call_site());
        self.code.extend(quote! {
            let (#var_nba, #var_auth_token) = {
                let ctx = #dal_context_builder.build_default_with_txns(#transactions);
                let r = ::dal_test::helpers::billing_account_signup(
                    &ctx,
                    #test_context.jwt_secret_key(),
                ).await?;
                #transactions = ctx.into();
                r
            };
        });
        self.billing_account_signup = Some((Arc::new(var_nba), Arc::new(var_auth_token)));

        self.billing_account_signup.as_ref().unwrap().clone()
    }

    fn setup_billing_account_id(&mut self) -> Arc<Ident> {
        if let Some(ref idents) = self.billing_account_id {
            return idents.clone();
        }

        let bas = self.setup_billing_account_signup();
        let nba = bas.0.as_ref();

        let var = Ident::new("nba_billing_account_id", Span::call_site());
        self.code.extend(quote! {
            let #var = *#nba.billing_account.id();
        });
        self.billing_account_id = Some(Arc::new(var));

        self.billing_account_id.as_ref().unwrap().clone()
    }

    fn setup_organization_id(&mut self) -> Arc<Ident> {
        if let Some(ref idents) = self.organization_id {
            return idents.clone();
        }

        let bas = self.setup_billing_account_signup();
        let nba = bas.0.as_ref();

        let var = Ident::new("nba_organization_id", Span::call_site());
        self.code.extend(quote! {
            let #var = *#nba.organization.id();
        });
        self.organization_id = Some(Arc::new(var));

        self.organization_id.as_ref().unwrap().clone()
    }

    fn setup_workspace_id(&mut self) -> Arc<Ident> {
        if let Some(ref idents) = self.workspace_id {
            return idents.clone();
        }

        let bas = self.setup_billing_account_signup();
        let nba = bas.0.as_ref();

        let var = Ident::new("nba_workspace_id", Span::call_site());
        self.code.extend(quote! {
            let #var = *#nba.workspace.id();
        });
        self.workspace_id = Some(Arc::new(var));

        self.workspace_id.as_ref().unwrap().clone()
    }

    fn setup_dal_context_default(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.dal_context_default {
            return ident.clone();
        }

        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();
        let transactions = self.setup_transactions();
        let transactions = transactions.as_ref();
        let bas = self.setup_billing_account_signup();
        let nba = bas.0.as_ref();

        let var = Ident::new("default_dal_context", Span::call_site());
        self.code.extend(quote! {
            let #var = {
                let mut ctx = #dal_context_builder.build_default_with_txns(#transactions.clone());
                ::dal_test::helpers::create_change_set_and_update_ctx(
                    &mut ctx,
                    &#nba,
                ).await;
                ctx
            };
        });
        self.dal_context_default = Some(Arc::new(var));

        self.dal_context_default.as_ref().unwrap().clone()
    }

    fn setup_dal_context_default_mut(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.dal_context_default_mut {
            return ident.clone();
        }

        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();
        let transactions = self.setup_transactions();
        let transactions = transactions.as_ref();
        let bas = self.setup_billing_account_signup();
        let nba = bas.0.as_ref();

        let var = Ident::new("dal_context_default_mut", Span::call_site());
        self.code.extend(quote! {
            let mut #var = {
                let mut ctx = #dal_context_builder.build_default_with_txns(#transactions.clone());
                ::dal_test::helpers::create_change_set_and_update_ctx(
                    &mut ctx,
                    &#nba,
                ).await;
                ctx
            };
        });
        self.dal_context_default_mut = Some(Arc::new(var));

        self.dal_context_default_mut.as_ref().unwrap().clone()
    }

    fn setup_dal_context_universal_head(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.dal_context_universal_head {
            return ident.clone();
        }

        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();
        let transactions = self.setup_transactions();
        let transactions = transactions.as_ref();

        let var = Ident::new("dal_context_universal_head", Span::call_site());
        self.code.extend(quote! {
            let #var = {
                let ctx = #dal_context_builder
                    .build_with_txns(
                        ::dal::RequestContext::new_universal_head(::dal::HistoryActor::SystemInit),
                        #transactions.clone(),
                    );
                ::dal_test::DalContextUniversalHead(ctx)
            };
        });
        self.dal_context_universal_head = Some(Arc::new(var));

        self.dal_context_universal_head.as_ref().unwrap().clone()
    }

    fn setup_dal_context_universal_head_ref(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.dal_context_universal_head_ref {
            return ident.clone();
        }

        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();
        let transactions = self.setup_transactions();
        let transactions = transactions.as_ref();

        let var = Ident::new("dal_context_universal_head_ref", Span::call_site());
        self.code.extend(quote! {
            let _dcuhr = {
                let ctx = #dal_context_builder
                    .build_with_txns(
                        ::dal::RequestContext::new_universal_head(::dal::HistoryActor::SystemInit),
                        #transactions.clone(),
                    );
                ctx
            };
            let #var = ::dal_test::DalContextUniversalHeadRef(&_dcuhr);
        });
        self.dal_context_universal_head_ref = Some(Arc::new(var));

        self.dal_context_universal_head_ref
            .as_ref()
            .unwrap()
            .clone()
    }

    fn setup_dal_context_universal_head_mut_ref(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.dal_context_universal_head_mut_ref {
            return ident.clone();
        }

        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();
        let transactions = self.setup_transactions();
        let transactions = transactions.as_ref();

        let var = Ident::new("dal_context_universal_head_mut_ref", Span::call_site());
        self.code.extend(quote! {
            let mut _dcuhmr = {
                let ctx = #dal_context_builder
                    .build_with_txns(
                        ::dal::RequestContext::new_universal_head(::dal::HistoryActor::SystemInit),
                        #transactions.clone(),
                    );
                ctx
            };
            let #var = ::dal_test::DalContextUniversalHeadMutRef(&mut _dcuhmr);
        });
        self.dal_context_universal_head_mut_ref = Some(Arc::new(var));

        self.dal_context_universal_head_mut_ref
            .as_ref()
            .unwrap()
            .clone()
    }

    fn setup_dal_context_head(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.dal_context_head {
            return ident.clone();
        }

        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();
        let transactions = self.setup_transactions();
        let transactions = transactions.as_ref();
        let bas = self.setup_billing_account_signup();
        let nba = bas.0.as_ref();

        let var = Ident::new("dal_context_head", Span::call_site());
        self.code.extend(quote! {
            let #var = {
                let mut ctx = #dal_context_builder
                    .build_with_txns(
                        ::dal::RequestContext::new_universal_head(::dal::HistoryActor::SystemInit),
                        #transactions.clone(),
                    );
                ctx
                    .update_to_workspace_tenancies(*#nba.workspace.id())
                    .await
                    .wrap_err("failed to update dal context to workspace tenancies")?;

                ::dal_test::DalContextHead(ctx)
            };
        });
        self.dal_context_head = Some(Arc::new(var));

        self.dal_context_head.as_ref().unwrap().clone()
    }

    fn setup_dal_context_head_ref(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.dal_context_head_ref {
            return ident.clone();
        }

        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();
        let transactions = self.setup_transactions();
        let transactions = transactions.as_ref();
        let bas = self.setup_billing_account_signup();
        let nba = bas.0.as_ref();

        let var = Ident::new("dal_context_head_ref", Span::call_site());
        self.code.extend(quote! {
            let _dchr = {
                let mut ctx = #dal_context_builder
                    .build_with_txns(
                        ::dal::RequestContext::new_universal_head(::dal::HistoryActor::SystemInit),
                        #transactions.clone(),
                    );
                ctx
                    .update_to_workspace_tenancies(*#nba.workspace.id())
                    .await
                    .wrap_err("failed to update dal context to workspace tenancies")?;
                ctx
            };
            let #var = ::dal_test::DalContextHeadRef(&_dchr);
        });
        self.dal_context_head_ref = Some(Arc::new(var));

        self.dal_context_head_ref.as_ref().unwrap().clone()
    }

    fn setup_dal_context_head_mut_ref(&mut self) -> Arc<Ident> {
        if let Some(ref ident) = self.dal_context_head_mut_ref {
            return ident.clone();
        }

        let dal_context_builder = self.setup_dal_context_builder();
        let dal_context_builder = dal_context_builder.as_ref();
        let transactions = self.setup_transactions();
        let transactions = transactions.as_ref();
        let bas = self.setup_billing_account_signup();
        let nba = bas.0.as_ref();

        let var = Ident::new("dal_context_head_mut_ref", Span::call_site());
        self.code.extend(quote! {
            let mut _dchmr = {
                let mut ctx = #dal_context_builder
                    .build_with_txns(
                        ::dal::RequestContext::new_universal_head(::dal::HistoryActor::SystemInit),
                        #transactions.clone(),
                    );
                ctx
                    .update_to_workspace_tenancies(*#nba.workspace.id())
                    .await
                    .wrap_err("failed to update dal context to workspace tenancies")?;
                ctx
            };
            let #var = ::dal_test::DalContextHeadMutRef(&mut _dchmr);
        });
        self.dal_context_head_mut_ref = Some(Arc::new(var));

        self.dal_context_head_mut_ref.as_ref().unwrap().clone()
    }

    fn drop_transactions_clone_if_created(&mut self) {
        if !self.has_transactions() {
            return;
        }

        let transactions = self.setup_transactions();
        let transactions = transactions.as_ref();

        self.code.extend(quote! {
            // Drop remaining clone so that no copies of the transaction can outlive a commit or
            // rollback
            drop(#transactions);
        });
    }

    fn has_args(&self) -> bool {
        !self.args.is_empty()
    }

    fn has_transactions(&self) -> bool {
        self.transactions.is_some()
    }

    fn finish(self) -> FnSetup {
        FnSetup {
            code: self.code,
            fn_args: self.args,
        }
    }
}

fn path_as_string(path: &Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn expand_color_eyre_init() -> TokenStream {
    quote! {
        ::dal_test::color_eyre::config::HookBuilder::default()
            .add_frame_filter(Box::new(|frames| {
                let mut displayed = ::std::collections::HashSet::new();
                let filters = &[
                    "tokio::",
                    "<futures_util::",
                    "std::panic",
                    "test::run_test_in_process",
                    "core::ops::function::FnOnce::call_once",
                    "std::thread::local",
                    "<core::future::",
                    "<alloc::boxed::Box",
                    "<std::panic::AssertUnwindSafe",
                    "core::result::Result",
                    "<T as futures_util",
                    "<tracing_futures::Instrumented",
                    "test::assert_test_result",
                    "spandoc::",
                ];

                frames.retain(|frame| {
                    let loc = (frame.lineno, &frame.filename);
                    let inserted = displayed.insert(loc);

                    if !inserted {
                        return false;
                    }

                    !filters.iter().any(|f| {
                        let name = if let Some(name) = frame.name.as_ref() {
                            name.as_str()
                        } else {
                            return true;
                        };

                        name.starts_with(f)
                    })
                });
            }))
            .install()
            .unwrap();
    }
}

fn expand_tracing_init() -> TokenStream {
    let span_events_env_var = SPAN_EVENTS_ENV_VAR;
    let log_env_var = LOG_ENV_VAR;

    quote! {
        let event_filter = {
            use ::dal_test::tracing_subscriber::fmt::format::FmtSpan;

            match ::std::env::var(#span_events_env_var) {
                Ok(value) => {
                    value
                        .to_ascii_lowercase()
                        .split(",")
                        .map(|filter| match filter.trim() {
                            "new" => FmtSpan::NEW,
                            "enter" => FmtSpan::ENTER,
                            "exit" => FmtSpan::EXIT,
                            "close" => FmtSpan::CLOSE,
                            "active" => FmtSpan::ACTIVE,
                            "full" => FmtSpan::FULL,
                            _ => panic!(
                                "{}: {} must contain filters separated by `,`.\n\t\
                                For example: `active` or `new,close`\n\t
                                Got: {}",
                                concat!(env!("CARGO_PKG_NAME"), "::dal_test"),
                                #span_events_env_var,
                                value,
                            ),
                        })
                        .fold(FmtSpan::NONE, |acc, filter| filter | acc)
                },
                Err(::std::env::VarError::NotUnicode(_)) => {
                    panic!(
                        "{}: {} must contain a valid UTF-8 string",
                        concat!(env!("CARGO_PKG_NAME"), "::dal_test"),
                        #span_events_env_var,
                    )
                }
                Err(::std::env::VarError::NotPresent) => FmtSpan::NONE,
            }
        };

        let subscriber = ::dal_test::tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(::dal_test::tracing_subscriber::EnvFilter::from_env(#log_env_var))
            .with_span_events(event_filter)
            .with_test_writer()
            .pretty()
            .finish();
        let _ = ::dal_test::telemetry::tracing::subscriber::set_global_default(subscriber);
    }
}

fn expand_runtime(worker_threads: usize, thread_stack_size: usize) -> TokenStream {
    quote! {
        ::tokio::runtime::Builder::new_multi_thread()
            .worker_threads(#worker_threads)
            .thread_stack_size(#thread_stack_size)
            .enable_all()
            .build()
            .expect("Failed building the Runtime")
    }
}
