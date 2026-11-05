/* Halcyon activity relay. Halcyon 0.2.0 to 0.6.0 post to this Apps Script
 * URL; this script forwards each report unchanged to the Cloudflare collector
 * and returns its answer. It stores nothing and never logs bodies.
 * Deploy as owner, access ANYONE_ANONYMOUS, at the existing deployment so the
 * URL stays the same. Run checkRelay() once in the editor to grant the
 * external-request permission.
 */
const COLLECTOR = 'https://halcyon-reports.profilaz3.workers.dev/v1/report';
const MAX_BODY = 1024;

function json_(value) {
  return ContentService.createTextOutput(JSON.stringify(value)).setMimeType(ContentService.MimeType.JSON);
}

// Same health answer as the collector, so release checks keep working.
function doGet() { return json_({service: 'halcyon-activity', schema: 6}); }

function doPost(event) {
  try {
    const body = event && event.postData && event.postData.contents;
    if (typeof body !== 'string' || body.length > MAX_BODY) return json_({ok: false});
    const response = UrlFetchApp.fetch(COLLECTOR, {
      method: 'post', contentType: 'application/json', payload: body,
      muteHttpExceptions: true, followRedirects: false,
    });
    if (response.getResponseCode() !== 200) return json_({ok: false});
    const text = response.getContentText();
    if (text.length > MAX_BODY) return json_({ok: false});
    const ack = JSON.parse(text);
    if (!ack || typeof ack !== 'object' || Array.isArray(ack) || typeof ack.ok !== 'boolean') return json_({ok: false});
    return json_(ack);
  } catch (_) {
    return json_({ok: false});
  }
}

// The former daily maintenance trigger may still call this; storage and
// retention now run on Cloudflare, so there is nothing left to do here.
function maintenance() {}

// Run once from the editor: grants permission and shows the collector health.
function checkRelay() {
  const text = UrlFetchApp.fetch(COLLECTOR, {muteHttpExceptions: true}).getContentText();
  Logger.log(text);
  return text;
}

if (typeof module !== 'undefined') module.exports = {doGet, doPost, maintenance, COLLECTOR};
