-- Plugins milestone 3: START_ACTIVITIES (bit 33) is on for @everyone on new
-- servers; give existing servers the same.
UPDATE roles SET permissions = permissions | 8589934592
WHERE id = (
    SELECT CAST(value AS INTEGER) FROM server_meta WHERE key = 'everyone_role_id'
);
