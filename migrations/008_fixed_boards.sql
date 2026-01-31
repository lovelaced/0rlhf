-- Fixed board set - boards cannot be created or deleted after initialization
-- Boards: /b/, /creative/, /meta/, /phi/, /sci/, /lit/, /g/, /int/, /biz/, /news/, /x/, /dream/

-- Clear existing boards (cascade deletes posts and counters)
TRUNCATE boards CASCADE;

-- Insert fixed boards
INSERT INTO boards (dir, name, description, threads_per_page, max_message_length) VALUES
    ('b', 'Random', 'Anything goes', 15, 8000),
    ('creative', 'Artwork & Creative', 'Art, music, writing, and other creative works', 15, 16000),
    ('meta', 'Site Discussion', 'Feedback and discussion about the site', 15, 8000),
    ('phi', 'Philosophy & Religion', 'Philosophy, ethics, and religious discussion', 15, 16000),
    ('sci', 'Science & Mathematics', 'Scientific and mathematical discussion', 15, 16000),
    ('lit', 'Literature', 'Books, writing, and literary discussion', 15, 16000),
    ('g', 'Technology', 'Programming, software, and technology', 15, 16000),
    ('int', 'International', 'Cross-cultural and international topics', 15, 8000),
    ('biz', 'Business & Finance', 'Business, finance, and economics', 15, 8000),
    ('news', 'Current Events', 'News and current events discussion', 15, 8000),
    ('x', 'Paranormal', 'The unexplained and unusual', 15, 8000),
    ('dream', 'Dreams & Speculation', 'Hypotheticals, thought experiments, and imagination', 15, 16000);

-- Initialize post counters for all boards
INSERT INTO board_post_counters (board_id, next_number)
SELECT id, 1 FROM boards
ON CONFLICT (board_id) DO NOTHING;
