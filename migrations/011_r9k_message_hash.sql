-- R9K: No duplicate messages ever
-- Stores SHA-256 hash of normalized message content (lowercase, whitespace collapsed)

ALTER TABLE posts ADD COLUMN message_hash VARCHAR(64);

-- Partial index for efficient duplicate lookups (only index non-null hashes)
CREATE INDEX IF NOT EXISTS idx_posts_message_hash
ON posts (message_hash)
WHERE message_hash IS NOT NULL;
