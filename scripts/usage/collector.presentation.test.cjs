const test = require('node:test');
const assert = require('node:assert/strict');
const {fixture} = require('./collector.fixture.cjs');

for (const operation of ['setup', 'maintenance', 'activity', 'opt-out']) {
  test(`reader views and Versions controls survive collector ${operation}`, () => {
    const f = fixture(), s = f.sandbox;
    s.setup();
    const readerCells = [];
    for (const name of ['Overview', 'Activity', 'History']) {
      const sheet = f.add(name);
      sheet.put(1, 26, 'halcyon:presentation:v1');
      sheet.put(5, 1, 0, '=Dashboard!B7');
      sheet.put(4, 2, 'Past 7 days');
      sheet.put(40, 8, 'Owner note');
      readerCells.push([sheet, JSON.stringify([...sheet.cells])]);
    }
    const versions = f.sheets.get('Versions');
    versions.put(1, 26, 'halcyon:presentation:v1');
    versions.put(4, 6, 'Past 7 days');
    versions.put(8, 5, 'calculated', '=FILTER(A5:C25,A5:A25<>"")');
    versions.put(60, 12, 'Owner note');
    const outside = () => JSON.stringify([...versions.cells].filter(([k]) => +k.split(',')[1] > 4));
    const before = outside();
    const activity = {schema:4,kind:'activity',installation:'a'.repeat(64),revision:0,version:'0.4.1',country:null,first_started_at:null};
    if (operation === 'setup' || operation === 'maintenance') s[operation]();
    else {
      assert.equal(s.doPost({postData:{contents:JSON.stringify(activity)}}).ok,true);
      if (operation === 'opt-out') assert.equal(s.doPost({postData:{contents:JSON.stringify({schema:3,kind:'status',installation:activity.installation,revision:1,enabled:false,changed_at:Math.floor(Date.now()/1000)})}}).ok,true);
    }
    for (const [sheet, cells] of readerCells) assert.equal(JSON.stringify([...sheet.cells]), cells);
    assert.equal(outside(), before);
    assert.equal(versions.cell(1,1).value,'halcyon:analytics:v1');
    assert.equal(versions.cell(4,1).value,'Version');
    assert.equal(s.analyticsPreflight_({getSheetByName:n=>f.sheets.get(n)}).Versions.length,60);
  });
}
