// Copyright 2021 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use common_base::infallible::RwLock;
use common_base::mem_allocator::malloc_size;
use common_exception::ErrorCode;
use common_exception::Result;
use common_macros::MallocSizeOf;
use common_meta_types::GrantObject;
use common_meta_types::UserInfo;
use common_meta_types::UserPrivilegeType;
use futures::channel::*;
use opendal::Operator;

use crate::catalogs::CatalogManager;
use crate::sessions::QueryContext;
use crate::sessions::QueryContextShared;
use crate::sessions::SessionContext;
use crate::sessions::SessionManager;
use crate::sessions::SessionStatus;
use crate::sessions::SessionType;
use crate::sessions::Settings;
use crate::users::RoleCacheMgr;
use crate::Config;

#[derive(MallocSizeOf)]
pub struct Session {
    pub(in crate::sessions) id: String,
    #[ignore_malloc_size_of = "insignificant"]
    pub(in crate::sessions) typ: RwLock<SessionType>,
    #[ignore_malloc_size_of = "insignificant"]
    pub(in crate::sessions) session_mgr: Arc<SessionManager>,
    pub(in crate::sessions) ref_count: Arc<AtomicUsize>,
    pub(in crate::sessions) session_ctx: Arc<SessionContext>,
    #[ignore_malloc_size_of = "insignificant"]
    session_settings: Settings,
    #[ignore_malloc_size_of = "insignificant"]
    status: Arc<RwLock<SessionStatus>>,
    pub(in crate::sessions) mysql_connection_id: Option<u32>,
}

impl Session {
    pub async fn try_create(
        conf: Config,
        id: String,
        typ: SessionType,
        session_mgr: Arc<SessionManager>,
        mysql_connection_id: Option<u32>,
    ) -> Result<Arc<Session>> {
        let session_ctx = Arc::new(SessionContext::try_create(conf.clone())?);
        let session_settings = Settings::try_create(&conf)?;
        let ref_count = Arc::new(AtomicUsize::new(0));
        let status = Arc::new(Default::default());
        Ok(Arc::new(Session {
            id,
            typ: RwLock::new(typ),
            session_mgr,
            ref_count,
            session_ctx,
            session_settings,
            status,
            mysql_connection_id,
        }))
    }

    pub fn get_mysql_conn_id(self: &Arc<Self>) -> Option<u32> {
        self.mysql_connection_id
    }

    pub fn get_id(self: &Arc<Self>) -> String {
        self.id.clone()
    }

    pub fn get_type(&self) -> SessionType {
        let lock = self.typ.read();
        lock.clone()
    }

    pub fn set_type(&self, typ: SessionType) {
        let mut lock = self.typ.write();
        *lock = typ;
    }

    pub fn is_aborting(self: &Arc<Self>) -> bool {
        self.session_ctx.get_abort()
    }

    pub fn quit(self: &Arc<Self>) {
        let session_ctx = self.session_ctx.clone();
        if session_ctx.get_current_query_id().is_some() {
            if let Some(io_shutdown) = session_ctx.take_io_shutdown_tx() {
                let (tx, rx) = oneshot::channel();
                if io_shutdown.send(tx).is_ok() {
                    // We ignore this error because the receiver is return cancelled error.
                    let _ = futures::executor::block_on(rx);
                }
            }
        }
        self.session_mgr.http_query_manager.kill_session(&self.id);
    }

    pub fn kill(self: &Arc<Self>) {
        self.session_ctx.set_abort(true);
        self.quit();
    }

    pub fn force_kill_session(self: &Arc<Self>) {
        self.force_kill_query();
        self.kill(/* shutdown io stream */);
    }

    pub fn force_kill_query(self: &Arc<Self>) {
        let session_ctx = self.session_ctx.clone();

        if let Some(context_shared) = session_ctx.take_query_context_shared() {
            context_shared.kill(/* shutdown executing query */);
        }
    }

    /// Create a query context for query.
    /// For a query, execution environment(e.g cluster) should be immutable.
    /// We can bind the environment to the context in create_context method.
    pub async fn create_query_context(self: &Arc<Self>) -> Result<Arc<QueryContext>> {
        let shared = self.get_shared_query_context().await?;

        Ok(QueryContext::create_from_shared(shared))
    }

    pub async fn get_shared_query_context(self: &Arc<Self>) -> Result<Arc<QueryContextShared>> {
        let discovery = self.session_mgr.get_cluster_discovery();

        let session = self.clone();
        let cluster = discovery.discover().await?;
        let shared = QueryContextShared::try_create(session, cluster).await?;
        self.session_ctx
            .set_query_context_shared(Some(shared.clone()));
        Ok(shared)
    }

    pub fn get_current_query_id(&self) -> Option<String> {
        self.session_ctx.get_current_query_id()
    }

    pub fn attach<F>(self: &Arc<Self>, host: Option<SocketAddr>, io_shutdown: F)
    where F: FnOnce() + Send + 'static {
        let (tx, rx) = oneshot::channel();
        self.session_ctx.set_client_host(host);
        self.session_ctx.set_io_shutdown_tx(Some(tx));

        common_base::base::tokio::spawn(async move {
            if let Ok(tx) = rx.await {
                (io_shutdown)();
                tx.send(()).ok();
            }
        });
    }

    pub fn set_current_database(self: &Arc<Self>, database_name: String) {
        self.session_ctx.set_current_database(database_name);
    }

    pub fn get_current_database(self: &Arc<Self>) -> String {
        self.session_ctx.get_current_database()
    }

    pub fn get_current_catalog(self: &Arc<Self>) -> String {
        self.session_ctx.get_current_catalog()
    }

    pub fn get_current_tenant(self: &Arc<Self>) -> String {
        self.session_ctx.get_current_tenant()
    }

    pub fn set_current_tenant(self: &Arc<Self>, tenant: String) {
        self.session_ctx.set_current_tenant(tenant);
    }

    pub fn get_current_user(self: &Arc<Self>) -> Result<UserInfo> {
        self.session_ctx
            .get_current_user()
            .ok_or_else(|| ErrorCode::AuthenticateFailure("unauthenticated"))
    }

    pub fn set_current_user(self: &Arc<Self>, user: UserInfo) {
        self.session_ctx.set_current_user(user);
    }

    pub async fn validate_privilege(
        self: &Arc<Self>,
        object: &GrantObject,
        privilege: UserPrivilegeType,
    ) -> Result<()> {
        let current_user = self.get_current_user()?;
        let user_verified = current_user.grants.verify_privilege(object, privilege);
        if user_verified {
            return Ok(());
        }

        let tenant = self.get_current_tenant();
        let role_cache = self
            .get_shared_query_context()
            .await?
            .get_role_cache_manager();
        let role_verified = role_cache
            .find_related_roles(&tenant, &current_user.grants.roles())
            .await?
            .iter()
            .any(|r| r.grants.verify_privilege(object, privilege));
        if role_verified {
            return Ok(());
        }

        Err(ErrorCode::PermissionDenied(format!(
            "Permission denied, user {} requires {} privilege on {}",
            &current_user.identity(),
            privilege,
            object
        )))
    }

    pub fn get_settings(self: &Arc<Self>) -> Arc<Settings> {
        Arc::new(self.session_settings.clone())
    }

    pub fn get_changed_settings(self: &Arc<Self>) -> Arc<Settings> {
        Arc::new(self.session_settings.get_changed_settings())
    }

    pub fn apply_changed_settings(self: &Arc<Self>, changed_settings: Arc<Settings>) -> Result<()> {
        self.session_settings
            .apply_changed_settings(changed_settings)
    }

    pub fn get_session_manager(self: &Arc<Self>) -> Arc<SessionManager> {
        self.session_mgr.clone()
    }

    pub fn get_catalogs(self: &Arc<Self>) -> Arc<CatalogManager> {
        self.session_mgr.get_catalog_manager()
    }

    pub fn get_memory_usage(self: &Arc<Self>) -> usize {
        malloc_size(self)
    }

    pub fn get_storage_operator(self: &Arc<Self>) -> Operator {
        self.session_mgr.get_storage_operator()
    }

    pub fn get_config(&self) -> Config {
        self.session_mgr.get_conf()
    }

    pub fn get_status(self: &Arc<Self>) -> Arc<RwLock<SessionStatus>> {
        self.status.clone()
    }

    pub fn get_role_cache_manager(self: &Arc<Self>) -> Arc<RoleCacheMgr> {
        self.session_mgr.get_role_cache_manager()
    }
}
