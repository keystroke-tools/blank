ALTER TABLE sites ADD COLUMN default_domain TEXT;
CREATE UNIQUE INDEX idx_sites_default_domain ON sites(default_domain) WHERE default_domain IS NOT NULL;
