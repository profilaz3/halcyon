-- Halcyon activity collector. Identifiable rows (installations, activity)
-- expire after 90 days; daily holds anonymous aggregates only.
CREATE TABLE installations (
  id TEXT PRIMARY KEY CHECK (length(id) = 64), -- SHA-256 of the random token
  status TEXT NOT NULL CHECK (status IN ('enabled', 'opted_out')),
  revision INTEGER NOT NULL CHECK (revision >= 0),
  choice_at TEXT,          -- client action time of the latest choice (informational)
  opted_out_at TEXT,       -- latest off action, kept on re-enable until aged out
  status_received_at TEXT, -- server receipt of the latest distinct choice
  country TEXT,            -- first reported country
  first_seen_at TEXT,      -- first activity received since tracking (server)
  first_seen_day INTEGER,  -- same, as a UTC epoch day
  first_started_at TEXT,   -- client-reported first desktop start, if known
  first_version TEXT,
  last_active_day INTEGER, -- UTC epoch day
  last_version TEXT,
  last_received_at TEXT
) STRICT;

-- At most one row per installation and UTC day: the last report of that day.
CREATE TABLE activity (
  id TEXT NOT NULL,
  day INTEGER NOT NULL,
  version TEXT,
  package_format TEXT NOT NULL,
  update_channel TEXT NOT NULL,
  installed_version TEXT,
  received_at TEXT,
  PRIMARY KEY (id, day)
) STRICT, WITHOUT ROWID;
CREATE INDEX activity_day ON activity (day);

-- Anonymous per-day totals, frozen once the day is complete.
CREATE TABLE daily (
  day INTEGER NOT NULL,
  segment TEXT NOT NULL,
  metric TEXT NOT NULL,
  key TEXT NOT NULL DEFAULT '',
  value INTEGER NOT NULL,
  PRIMARY KEY (day, segment, metric, key)
) STRICT, WITHOUT ROWID;

CREATE TABLE releases (
  version TEXT PRIMARY KEY,
  published_day INTEGER NOT NULL
) STRICT;

CREATE TABLE meta (
  key TEXT PRIMARY KEY,
  value TEXT
) STRICT;
