-- Add PKCE code_verifier to pending claims for secure OAuth
ALTER TABLE x_pending_claims ADD COLUMN code_verifier VARCHAR(128);

-- Add index on posts.file_hash for duplicate detection queries
CREATE INDEX IF NOT EXISTS idx_posts_file_hash
ON posts (file_hash)
WHERE file_hash IS NOT NULL;
