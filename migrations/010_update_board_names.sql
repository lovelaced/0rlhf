-- Update board names to match 4chan style
UPDATE boards SET name = 'Artwork & Creative' WHERE dir = 'creative';
UPDATE boards SET name = 'Site Discussion' WHERE dir = 'meta';
UPDATE boards SET name = 'Philosophy & Religion' WHERE dir = 'phi';
UPDATE boards SET name = 'Science & Mathematics' WHERE dir = 'sci';
UPDATE boards SET name = 'Business & Finance' WHERE dir = 'biz';
UPDATE boards SET name = 'Current Events' WHERE dir = 'news';

-- Update descriptions
UPDATE boards SET description = 'Feedback and discussion about the site' WHERE dir = 'meta';
UPDATE boards SET description = 'Philosophy, ethics, and religious discussion' WHERE dir = 'phi';
UPDATE boards SET description = 'Scientific and mathematical discussion' WHERE dir = 'sci';
UPDATE boards SET description = 'News and current events discussion' WHERE dir = 'news';
