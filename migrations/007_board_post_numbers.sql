-- Per-board post numbering (each board starts at 1)

-- Add post_number column to posts
ALTER TABLE posts ADD COLUMN post_number BIGINT;

-- Counter table for tracking next post number per board
CREATE TABLE board_post_counters (
    board_id INTEGER PRIMARY KEY REFERENCES boards(id) ON DELETE CASCADE,
    next_number BIGINT NOT NULL DEFAULT 1
);

-- Initialize counters for existing boards
INSERT INTO board_post_counters (board_id, next_number)
SELECT id, 1 FROM boards
ON CONFLICT (board_id) DO NOTHING;

-- Backfill post_number for existing posts (ordered by id within each board)
WITH numbered AS (
    SELECT id, board_id, ROW_NUMBER() OVER (PARTITION BY board_id ORDER BY id) as rn
    FROM posts
)
UPDATE posts SET post_number = numbered.rn
FROM numbered WHERE posts.id = numbered.id;

-- Update counters to reflect existing posts
UPDATE board_post_counters bc
SET next_number = COALESCE(
    (SELECT MAX(post_number) + 1 FROM posts WHERE board_id = bc.board_id),
    1
);

-- Make post_number NOT NULL after backfill
ALTER TABLE posts ALTER COLUMN post_number SET NOT NULL;

-- Unique constraint: post_number is unique per board
CREATE UNIQUE INDEX idx_posts_board_number ON posts (board_id, post_number);

-- Function to assign post_number on insert
CREATE OR REPLACE FUNCTION assign_post_number()
RETURNS TRIGGER AS $$
DECLARE
    new_number BIGINT;
BEGIN
    -- Atomically get and increment the counter
    UPDATE board_post_counters
    SET next_number = next_number + 1
    WHERE board_id = NEW.board_id
    RETURNING next_number - 1 INTO new_number;

    -- If no counter exists (new board), create it
    IF new_number IS NULL THEN
        INSERT INTO board_post_counters (board_id, next_number)
        VALUES (NEW.board_id, 2)
        ON CONFLICT (board_id) DO UPDATE SET next_number = board_post_counters.next_number + 1
        RETURNING next_number - 1 INTO new_number;
    END IF;

    NEW.post_number := new_number;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to auto-assign post_number
CREATE TRIGGER trg_assign_post_number
    BEFORE INSERT ON posts
    FOR EACH ROW
    EXECUTE FUNCTION assign_post_number();

-- Also create counter when board is created
CREATE OR REPLACE FUNCTION create_board_counter()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO board_post_counters (board_id, next_number)
    VALUES (NEW.id, 1)
    ON CONFLICT (board_id) DO NOTHING;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_create_board_counter
    AFTER INSERT ON boards
    FOR EACH ROW
    EXECUTE FUNCTION create_board_counter();
