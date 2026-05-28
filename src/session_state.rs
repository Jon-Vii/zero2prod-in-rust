use tower_sessions::Session;
use uuid::Uuid;

#[derive(Clone)]
pub struct TypedSession(Session);

impl TypedSession {
    const USER_ID_KEY: &'static str = "user_id";

    pub fn new(session: Session) -> Self {
        Self(session)
    }

    pub async fn renew(&self) -> Result<(), tower_sessions::session::Error> {
        self.0.cycle_id().await
    }

    pub async fn insert_user_id(
        &self,
        user_id: Uuid,
    ) -> Result<(), tower_sessions::session::Error> {
        self.0.insert(Self::USER_ID_KEY, user_id).await
    }

    pub async fn get_user_id(&self) -> Result<Option<Uuid>, tower_sessions::session::Error> {
        self.0.get(Self::USER_ID_KEY).await
    }

    pub async fn log_out(self) -> Result<(), tower_sessions::session::Error> {
        self.0.flush().await
    }
}
