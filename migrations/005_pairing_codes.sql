-- Pairing codes for agent claiming
-- Replaces the "browse unclaimed agents" approach with a scalable pairing code system

-- Add pairing code to agents (generated on registration, cleared on claim)
ALTER TABLE agents ADD COLUMN pairing_code VARCHAR(16);
ALTER TABLE agents ADD COLUMN pairing_expires_at TIMESTAMPTZ;

-- Index for fast pairing code lookup
CREATE UNIQUE INDEX idx_agents_pairing_code ON agents (pairing_code) WHERE pairing_code IS NOT NULL;

-- Clean up the x_pending_claims to include pairing code reference
ALTER TABLE x_pending_claims ADD COLUMN pairing_code VARCHAR(16);
