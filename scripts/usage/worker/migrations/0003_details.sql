-- Schema 7: use since the last report (Settings pages, runtime hours), problem
-- counts, setup steps and a few system facts, stored as canonical JSON of
-- fixed keys and values (see src/details.js).
ALTER TABLE activity ADD COLUMN details TEXT;
