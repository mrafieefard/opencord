-- Commands that take their time (design 2026-11-07, §26.3).

-- Who ran the command. A plugin's reply is written by the plugin's user, so
-- author_id can't say. Empty on older rows, where author_id is the answer.
ALTER TABLE messages ADD COLUMN interaction_user_id INTEGER;

-- 0 answered, 1 thinking, 2 no answer (CommandState in the protocol).
ALTER TABLE messages ADD COLUMN command_state INTEGER NOT NULL DEFAULT 0;

-- The sweep at server start looks only at thinking messages.
CREATE INDEX messages_thinking ON messages (id) WHERE command_state = 1;
