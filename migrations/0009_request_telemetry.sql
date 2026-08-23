ALTER TABLE site_request_logs ADD COLUMN ip_address TEXT;
ALTER TABLE site_request_logs ADD COLUMN country TEXT;
ALTER TABLE site_request_logs ADD COLUMN device_type TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE site_request_logs ADD COLUMN user_agent TEXT;
ALTER TABLE site_request_logs ADD COLUMN referer TEXT;
CREATE INDEX site_request_logs_site_status_idx ON site_request_logs(site_id, status);
