use std::sync::Arc;
use uuid::Uuid;

use crate::core::ports::storage::NamespaceRepository;
use crate::core::domain::namespace::{Namespace, NamespaceRole, NewNamespace};

pub struct NamespaceService{
    repo: Arc<dyn NamespaceRepository>,
}

impl NamespaceService {
    #[cold]
    pub fn new(r: Arc<dyn NamespaceRepository>) -> Self {
        NamespaceService{
            repo: r,
        }
    }

    pub async fn create(&self, uid: Uuid, ns: &NewNamespace) -> Result<Namespace, anyhow::Error> {
        self.repo.create(uid, ns).await
    }

    pub async fn find_all(&self, uid: Uuid) -> Result<Vec<Namespace>, anyhow::Error> {
        self.repo.find_by_uid(uid).await
    }

    pub async fn ns_role_by_uid(&self, uid: Uuid, ns_id: Uuid) -> Result<Option<NamespaceRole>, anyhow::Error> {
        self.repo.role_by_uid(uid, ns_id).await
    }
}