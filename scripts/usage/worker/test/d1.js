// Minimal D1 binding over node:sqlite for tests: prepare/bind/first/all/run/batch.
import {DatabaseSync} from 'node:sqlite';
import {readFileSync, readdirSync} from 'node:fs';

const MIGRATIONS = new URL('../migrations/', import.meta.url);

export function createD1() {
  const db = new DatabaseSync(':memory:');
  for (const file of readdirSync(MIGRATIONS).sort()) db.exec(readFileSync(new URL(file, MIGRATIONS), 'utf8'));
  const plain = row => row === undefined ? null : {...row};
  const statement = (sql, params = []) => ({
    sql, params,
    bind: (...values) => {
      if (values.some(v => v === undefined || typeof v === 'boolean')) throw TypeError('Unsupported D1 binding');
      return statement(sql, values);
    },
    first: async column => { const row = plain(db.prepare(sql).get(...params)); return row && column ? row[column] : row; },
    all: async () => ({success: true, results: db.prepare(sql).all(...params).map(plain), meta: {}}),
    run: async () => execute(sql, params),
  });
  const execute = (sql, params) => {
    const prepared = db.prepare(sql);
    if (/^\s*SELECT/i.test(sql)) return {success: true, results: prepared.all(...params).map(plain), meta: {changes: 0}};
    const result = prepared.run(...params);
    return {success: true, results: [], meta: {changes: Number(result.changes)}};
  };
  return {
    raw: db,
    prepare: sql => statement(sql),
    batch: async statements => {
      db.exec('BEGIN');
      try {
        const out = statements.map(s => execute(s.sql, s.params));
        db.exec('COMMIT');
        return out;
      } catch (error) { db.exec('ROLLBACK'); throw error; }
    },
  };
}
