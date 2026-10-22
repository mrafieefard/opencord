-- Phase 3: chat commands. A command's public result is a message by who
-- ran it, marked with the command; NULL for every other message.
ALTER TABLE messages ADD COLUMN interaction_command TEXT;
