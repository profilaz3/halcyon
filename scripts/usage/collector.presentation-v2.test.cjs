'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const {build,OWNER}=require('./presentation-v2.cjs');
function fixture(){
 const names=['Overview','Activity','Versions','History'];
 return {
  metadata:{sheets:names.map((title,i)=>({properties:{title,sheetId:i+1},charts:[{chartId:804050101+i},{chartId:77+i}]}))},
  inspected:{sheets:names.map(title=>({properties:{title},data:[{startColumn:25,rowData:[{values:[{userEnteredValue:{stringValue:OWNER}}]}]}]}))}
 };
}
test('presentation migration refuses missing or foreign ownership',()=>{
 for(const owner of [undefined,'personal sheet']){
  const f=fixture();f.inspected.sheets[2].data[0].rowData[0].values[0].userEnteredValue.stringValue=owner;
  assert.throws(()=>build(f.metadata,f.inspected),/Unowned reader: Versions/);
 }
});
test('presentation preserves collector-owned Versions cells and foreign charts',()=>{
 const f=fixture(),{requests}=build(f.metadata,f.inspected);
 for(const q of requests){
  for(const op of ['updateCells','repeatCell','mergeCells','unmergeCells','setDataValidation']){
   const r=q[op]?.range;if(r?.sheetId===3)assert.ok(r.startColumnIndex>=4,op);
  }
  if(q.deleteEmbeddedObject)assert.ok(q.deleteEmbeddedObject.objectId>=804050100);
 }
});
test('reader charts are created after their formulas, with every numeric series',()=>{
 const f=fixture(),{requests}=build(f.metadata,f.inspected),charts=requests.filter(q=>q.addChart);
 assert.equal(charts.length,4);
 assert.deepEqual(requests.slice(-4),charts);
 assert.deepEqual(charts.map(q=>q.addChart.chart.spec.basicChart.series.length),[1,1,2,1]);
});
