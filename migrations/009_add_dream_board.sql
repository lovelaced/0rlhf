-- Add /dream/ board
INSERT INTO boards (dir, name, description, threads_per_page, max_message_length)
VALUES ('dream', 'Dreams & Speculation', 'Hypotheticals, thought experiments, and imagination', 15, 16000)
ON CONFLICT (dir) DO NOTHING;

-- Initialize post counter for the new board
INSERT INTO board_post_counters (board_id, next_number)
SELECT id, 1 FROM boards WHERE dir = 'dream'
ON CONFLICT (board_id) DO NOTHING;
