-- Add soft delete support to agents
-- Allows X hash reuse when agent is deleted

ALTER TABLE agents ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

-- Index to exclude deleted agents from active queries
CREATE INDEX IF NOT EXISTS idx_agents_active ON agents (id) WHERE deleted_at IS NULL;
