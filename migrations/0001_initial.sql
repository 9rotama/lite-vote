CREATE TABLE voting_rooms (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    question TEXT NOT NULL,
    participant_names_public INTEGER NOT NULL,
    creator_token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    closed_at TEXT
);

CREATE TABLE choices (
    room_id INTEGER NOT NULL,
    id INTEGER NOT NULL,
    text TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (room_id, id),
    UNIQUE (room_id, text),
    UNIQUE (room_id, position),
    FOREIGN KEY (room_id) REFERENCES voting_rooms (id)
);

CREATE TABLE participants (
    room_id INTEGER NOT NULL,
    id INTEGER NOT NULL,
    token_hash TEXT NOT NULL,
    display_name TEXT,
    PRIMARY KEY (room_id, id),
    UNIQUE (room_id, token_hash),
    FOREIGN KEY (room_id) REFERENCES voting_rooms (id)
);

CREATE TABLE votes (
    room_id INTEGER NOT NULL,
    participant_id INTEGER NOT NULL,
    choice_id INTEGER NOT NULL,
    PRIMARY KEY (room_id, participant_id),
    FOREIGN KEY (room_id, participant_id)
        REFERENCES participants (room_id, id),
    FOREIGN KEY (room_id, choice_id)
        REFERENCES choices (room_id, id)
);

CREATE INDEX choices_room_idx ON choices (room_id);
CREATE INDEX participants_room_idx ON participants (room_id);
CREATE INDEX votes_choice_idx ON votes (room_id, choice_id);
