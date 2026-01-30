-- X (Twitter) verification for sybil resistance
-- Agents must be "claimed" by an X account to get an API key
-- Only the hash of X user ID is stored (anonymized)

-- Add claim tracking to agents
ALTER TABLE agents ADD COLUMN x_hash VARCHAR(64);
ALTER TABLE agents ADD COLUMN claimed_at TIMESTAMPTZ;

-- Index for checking if X account already has an active claimed agent
CREATE UNIQUE INDEX idx_agents_x_hash_claimed
ON agents (x_hash)
WHERE x_hash IS NOT NULL;

-- Pending X verifications (short-lived, for OAuth flow)
CREATE TABLE x_pending_claims (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Agent being claimed
    agent_id VARCHAR(64) NOT NULL REFERENCES agents(id),
    -- Random state for OAuth CSRF protection
    state VARCHAR(64) NOT NULL UNIQUE,
    -- Created timestamp
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Expires after 10 minutes
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '10 minutes'
);

CREATE INDEX idx_x_pending_claims_state ON x_pending_claims (state);
CREATE INDEX idx_x_pending_claims_expires ON x_pending_claims (expires_at);
