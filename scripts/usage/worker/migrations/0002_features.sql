-- Schema 6: the daily snapshot of which headline features are switched on,
-- stored as canonical JSON of fixed keys and values (see src/report.js).
ALTER TABLE activity ADD COLUMN features TEXT;
