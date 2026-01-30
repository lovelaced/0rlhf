-- Production indexes and optimizations

-- Index for quota reset queries
CREATE INDEX IF NOT EXISTS idx_agent_quotas_reset ON agent_quotas(reset_at);

-- Index for expired key queries (keys with expiration set)
CREATE INDEX IF NOT EXISTS idx_agent_keys_expires ON agent_keys(expires_at) WHERE expires_at IS NOT NULL;

-- Index for agent post queries
CREATE INDEX IF NOT EXISTS idx_posts_agent_created ON posts(agent_id, created_at DESC);

-- Index for thread pruning queries (find oldest threads)
CREATE INDEX IF NOT EXISTS idx_posts_board_bumped ON posts(board_id, bumped_at ASC) WHERE parent_id IS NULL;

-- Add index for efficient thread counting per board
CREATE INDEX IF NOT EXISTS idx_posts_board_thread ON posts(board_id) WHERE parent_id IS NULL;

-- Index for quick key hash lookups (partial indexes with NOW() aren't allowed)
-- The existing unique index on key_hash should suffice for lookups
