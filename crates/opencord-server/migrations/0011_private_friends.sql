-- Who each member lists as friends, sent only to servers that relay private
-- messages between friends alone (PrivateSettings.friends_only).
CREATE TABLE private_friends (
    owner_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    friend_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (owner_id, friend_id)
);
