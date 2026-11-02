-- IANA timezone name (e.g. "Asia/Taipei"). NULL means "not chosen yet" and is
-- treated as UTC, so existing users keep today's behaviour until they pick one.
ALTER TABLE users ADD COLUMN timezone TEXT;
