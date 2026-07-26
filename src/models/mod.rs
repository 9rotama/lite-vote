#[derive(Debug, toasty::Model)]
pub struct VotingRoom {
    #[key]
    pub id: i64,
    #[unique]
    pub slug: String,
    pub question: String,
    pub participant_names_public: bool,
    #[unique]
    pub creator_token_hash: String,
    pub created_at: String,
    pub closed_at: Option<String>,
    #[has_many(pair = room)]
    pub choices: toasty::Deferred<Vec<Choice>>,
    #[has_many(pair = room)]
    pub participants: toasty::Deferred<Vec<Participant>>,
}

impl VotingRoom {
    pub fn is_closed(&self) -> bool {
        self.closed_at.is_some()
    }
}

#[derive(Debug, toasty::Model)]
#[allow(clippy::duplicated_attributes)]
#[key(room_id, id)]
#[unique(room_id, position)]
#[unique(room_id, text)]
pub struct Choice {
    pub room_id: i64,
    pub id: i64,
    pub text: String,
    pub position: i64,
    #[belongs_to(key = room_id, references = id)]
    pub room: toasty::Deferred<VotingRoom>,
    #[has_many(pair = choice)]
    pub votes: toasty::Deferred<Vec<Vote>>,
}

#[derive(Debug, toasty::Model)]
#[key(room_id, id)]
#[unique(room_id, token_hash)]
pub struct Participant {
    pub room_id: i64,
    pub id: i64,
    pub token_hash: String,
    pub display_name: Option<String>,
    #[belongs_to(key = room_id, references = id)]
    pub room: toasty::Deferred<VotingRoom>,
    #[has_one(pair = participant)]
    pub vote: toasty::Deferred<Option<Vote>>,
}

#[derive(Debug, toasty::Model)]
#[key(room_id, participant_id)]
#[index(room_id, choice_id)]
pub struct Vote {
    pub room_id: i64,
    pub participant_id: i64,
    pub choice_id: i64,
    #[belongs_to(key = [room_id, participant_id], references = [room_id, id])]
    pub participant: toasty::Deferred<Participant>,
    #[belongs_to(key = [room_id, choice_id], references = [room_id, id])]
    pub choice: toasty::Deferred<Choice>,
}
