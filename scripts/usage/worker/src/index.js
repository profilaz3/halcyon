// Halcyon reports: activity collector, hourly maintenance and the public
// aggregate API. The dashboard page itself is served from ./public.
// Never log request bodies, tokens or installation hashes.

import {MAX_BODY, applyReport, installationId, validateReport} from './report.js';
import {maintain} from './stats.js';

const HEALTH = {service: 'halcyon-activity', schema: 7};
const SECURITY_HEADERS = {
  'X-Content-Type-Options': 'nosniff',
  'Referrer-Policy': 'no-referrer',
};

const json = (value, init = {}) => new Response(JSON.stringify(value), {
  ...init,
  headers: {'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store', ...SECURITY_HEADERS, ...init.headers},
});

async function readBody(request) {
  const declared = Number(request.headers.get('Content-Length'));
  if (Number.isFinite(declared) && declared > MAX_BODY) return null;
  if (!request.body) return '';
  const reader = request.body.getReader();
  const chunks = [];
  let size = 0;
  for (;;) {
    const {done, value} = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > MAX_BODY) { await reader.cancel(); return null; }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const c of chunks) { bytes.set(c, offset); offset += c.byteLength; }
  return new TextDecoder('utf-8', {fatal: true}).decode(bytes);
}

export async function handleReport(request, env, now = new Date()) {
  try {
    const body = await readBody(request);
    if (body === null) return json({ok: false});
    const report = validateReport(body);
    const id = await installationId(report.installation);
    return json(await applyReport(env.DB, id, report, now));
  } catch (_) {
    // Same contract as the Apps Script collector: HTTP 200 with ok:false.
    return json({ok: false});
  }
}

async function summary(env) {
  const row = await env.DB.prepare(`SELECT value FROM meta WHERE key = 'summary'`).first();
  if (!row) return json({error: 'not ready'}, {status: 503, headers: {'Retry-After': '3600'}});
  return new Response(row.value, {
    headers: {
      'Content-Type': 'application/json; charset=utf-8',
      'Cache-Control': 'public, max-age=300',
      'Access-Control-Allow-Origin': '*',
      ...SECURITY_HEADERS,
    },
  });
}

export default {
  async fetch(request, env) {
    const {pathname} = new URL(request.url);
    if (pathname === '/v1/report') {
      if (request.method === 'POST') return handleReport(request, env);
      if (request.method === 'GET' || request.method === 'HEAD') return json(HEALTH);
      return json({ok: false}, {status: 405, headers: {Allow: 'GET, POST'}});
    }
    if (pathname === '/api/summary.json' && (request.method === 'GET' || request.method === 'HEAD')) return summary(env);
    return new Response('Not found', {status: 404, headers: SECURITY_HEADERS});
  },
  async scheduled(_event, env, ctx) {
    ctx.waitUntil(maintain(env.DB, new Date()));
  },
};
