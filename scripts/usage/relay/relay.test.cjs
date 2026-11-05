const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function relay(respond) {
  const calls = [];
  const context = {
    module: {exports: {}},
    Logger: {log() {}},
    ContentService: {MimeType: {JSON: 'json'}, createTextOutput: text => ({text, setMimeType() { return JSON.parse(text); }})},
    UrlFetchApp: {fetch(url, options) { calls.push({url, options}); return respond(url, options); }},
  };
  vm.runInNewContext(fs.readFileSync(__dirname + '/Code.gs', 'utf8'), context);
  return {...context.module.exports, calls};
}
const reply = (code, text) => () => ({getResponseCode: () => code, getContentText: () => text});
const body = JSON.stringify({schema: 1, installation: 'a'.repeat(64), version: '0.2.0'});

test('forwards the exact body and returns the collector acknowledgement', () => {
  const r = relay(reply(200, '{"ok":true,"day":"2026-11-05"}'));
  assert.deepEqual(r.doPost({postData: {contents: body}}), {ok: true, day: '2026-11-05'});
  assert.equal(r.calls.length, 1);
  assert.equal(r.calls[0].url, 'https://halcyon-reports.profilaz3.workers.dev/v1/report');
  assert.equal(r.calls[0].options.payload, body);
  assert.equal(r.calls[0].options.followRedirects, false);
});

test('answers ok:false for oversized bodies, errors and malformed replies', () => {
  assert.deepEqual(relay(reply(200, '{"ok":true}')).doPost({postData: {contents: ' '.repeat(1025)}}), {ok: false});
  assert.deepEqual(relay(reply(500, '{"ok":true}')).doPost({postData: {contents: body}}), {ok: false});
  assert.deepEqual(relay(reply(200, '<html>')).doPost({postData: {contents: body}}), {ok: false});
  assert.deepEqual(relay(reply(200, '[1]')).doPost({postData: {contents: body}}), {ok: false});
  assert.deepEqual(relay(() => { throw new Error('offline'); }).doPost({postData: {contents: body}}), {ok: false});
  assert.deepEqual(relay(reply(200, '{}')).doPost({}), {ok: false});
});

test('keeps the health answer the release check expects and a harmless trigger target', () => {
  const r = relay(reply(200, '{}'));
  assert.deepEqual(r.doGet(), {service: 'halcyon-activity', schema: 6});
  assert.equal(r.maintenance(), undefined);
  assert.equal(r.calls.length, 0);
});
