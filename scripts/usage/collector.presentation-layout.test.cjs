const test=require('node:test');
const assert=require('node:assert/strict');
const {build,NEW_IDS}=require('./presentation.cjs');
const names=['Dashboard','Versions','Daily activity','Installation history','Countries','Summary','Reporting','Installations'];
const metadata={sheets:names.map((title,sheetId)=>({properties:{title,sheetId,gridProperties:{rowCount:1000,columnCount:26}}}))};
const inspected={sheets:[{properties:{title:'Versions'},data:[{rowData:[{values:[{userEnteredValue:{stringValue:'halcyon:analytics:v1'}}]}]}]}]};
test('presentation is additive and never writes collector-owned cells',()=>{
 const result=build(metadata,inspected),newIds=new Set(Object.values(NEW_IDS));
 for(const request of [...result.stage,...result.publish]) {
  assert.equal(Object.keys(request).length,1);
  assert.equal(request.deleteSheet,undefined);assert.equal(request.deleteDimension,undefined);
  if(!request.updateCells) continue;
  const {range,rows,fields}=request.updateCells;
  assert.equal(rows.length,range.endRowIndex-range.startRowIndex);
  rows.forEach(r=>assert.equal(r.values.length,range.endColumnIndex-range.startColumnIndex));
  assert.ok(newIds.has(range.sheetId)||(range.sheetId===1&&range.startColumnIndex>=4));
  assert.match(fields,/userEnteredValue/);
 }
 assert.deepEqual(result.publish.filter(r=>r.updateSheetProperties?.properties.hidden===false).map(r=>r.updateSheetProperties.properties.sheetId),[NEW_IDS.Overview,NEW_IDS.Activity,1,NEW_IDS.History]);
});
test('existing views, ID collisions and owner contents are never overwritten',()=>{
 assert.throws(()=>build({...metadata,sheets:[...metadata.sheets,{properties:{title:'Overview',sheetId:900}}]},inspected));
 assert.throws(()=>build({...metadata,sheets:[...metadata.sheets,{properties:{title:'Unrelated',sheetId:NEW_IDS.Activity}}]},inspected));
 for(const occupied of [{userEnteredValue:{formulaValue:'=""'}},{note:'Owner note'},{dataValidation:{}}]) {
  const cells={sheets:[{properties:{title:'Versions'},data:[{startColumn:4,rowData:[{values:[occupied]}]}]}]};
  assert.throws(()=>build(metadata,cells),/Occupied/);
 }
});
test('controls use bounded source ranges and unknown history stays unknown',()=>{
 const result=build(metadata,inspected),formulas=result.stage.flatMap(r=>(r.updateCells?.rows||[]).flatMap(row=>row.values.map(c=>c.userEnteredValue?.formulaValue).filter(Boolean)));
 assert.ok(formulas.some(f=>f.includes('A5:A20004')));
 assert.ok(formulas.some(f=>f.includes('ISNUMBER(\'Daily activity\'!B5:B94)')));
 assert.ok(formulas.some(f=>f.includes('"Unknown","Unknown"')));
 assert.ok(formulas.some(f=>f.includes('Record no longer retained')));
 assert.ok(formulas.every(f=>!/(IMPORT|WEBSERVICE|GOOGLEFINANCE)/.test(f)));
 const controls=result.stage.filter(r=>r.setDataValidation);
 assert.equal(controls.length,4);
 assert.ok(controls.every(r=>r.setDataValidation.rule.strict));
});
