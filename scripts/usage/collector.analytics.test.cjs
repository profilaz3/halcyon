const test=require('node:test');
const assert=require('node:assert/strict');
const {fixture}=require('./collector.fixture.cjs');
const now=new Date('2026-10-28T12:00:00Z');
const report={schema:4,kind:'activity',installation:'a'.repeat(64),revision:0,version:'0.4.0',country:null,first_started_at:null};
const date=offset=>new Date(+now+offset*86400000);
const plain=v=>JSON.parse(JSON.stringify(v));
function observe(s,rows,id,offset,version='0.4.0') {return s.applyReport(rows,id,{...report,version},date(offset));}
function metric(result,name) {return result.Dashboard.find(r=>r[0]===name)[1];}

test('existing reports create bounded version observations without a new client schema',()=>{
 const {sandbox:s}=fixture(),today=s.dayNumber(now);
 let rows=observe(s,[],'one',-4);rows=observe(s,rows,'one',-2,'0.4.1');
 rows=observe(s,rows,'one',-2,'0.4.1');rows=observe(s,rows,'one',-2,'0.4.2');
 rows=observe(s,rows,'one',0,'0.4.2');
 assert.deepEqual(plain(s.versions_(rows[0][15])),[[today-4,'0.4.0'],[today-2,'0.4.2']]);
 assert.equal(rows[0][11],'0.4.0');assert.equal(rows[0][12],3);
 const aged=s.currentRows(rows,today+88);
 assert.deepEqual(plain(s.versions_(aged[0][15])),[]);
 for(let i=1;i<110;i++) rows=observe(s,rows,'one',i,`0.4.${i}`);
 assert.equal(s.versions_(rows[0][15]).length,90);
});
test('same-day duplicate rows retain the latest received version and merge older observations',()=>{
 const {sandbox:s}=fixture();
 const a=observe(s,[],'one',-2);
 const b=s.applyReport(a,'one',{...report,version:'0.4.1'},new Date(+date(-2)+3600000));
 for(const rows of [[a[0],b[0]],[b[0],a[0]]]) {
  const merged=s.currentRows(rows,s.dayNumber(now));
  assert.equal(merged.length,1);assert.equal(merged[0][2],'0.4.1');
  assert.equal(s.versions_(merged[0][15])[0][1],'0.4.1');
 }
});
test('legacy migration leaves unknown history unknown and never rewrites first version',()=>{
 const f=fixture(),s=f.sandbox,today=s.dayNumber(new Date());
 f.seed([['legacy',today,'0.2.5','HR']]);s.setup();s.setup();
 assert.equal(f.rows()[1][15],'');
 const dashboard=s.analytics_(f.rows().slice(1),today);
 assert.equal(metric(dashboard,'Active today'),1);
 assert.equal(metric(dashboard,'Active with unknown first seen'),1);
 assert.equal(metric(dashboard,'Returned in week 2'),'Not enough history');
 assert.ok(dashboard['Daily activity'].every(r=>r[1]===''));
 assert.equal(dashboard['Installation history'][0][3],'Unknown');
});
test('daily, new, returning, version and repeat-use counts use distinct enabled installations',()=>{
 const {sandbox:s}=fixture(),today=s.dayNumber(now);let rows=[];
 for(const [id,day,v] of [['one',-20,'0.4.0'],['one',-5,'0.4.1'],['two',-2,'0.4.1'],['off',-1,'0.3.0'],['one',0,'0.4.1']])
  rows=observe(s,rows,id,day,v);
 rows=s.applyReport(rows,'off',{kind:'status',revision:1,enabled:false,changed_at:+now/1000},now);
 rows=s.applyReport(rows,'status-only',{kind:'status',revision:1,enabled:true,changed_at:+now/1000},now);
 const a=s.analytics_([...rows,...rows],today);
 assert.equal(metric(a,'Active today'),1);assert.equal(metric(a,'Active in 7 days'),2);
 assert.equal(metric(a,'First seen in 7 days'),1);assert.equal(metric(a,'Returning in 7 days'),1);
 assert.equal(metric(a,'Used on 2+ days this week'),1);assert.equal(metric(a,'Currently opted out'),1);
 assert.deepEqual(plain(a.Versions),[['0.4.1',2,2,1]]);
 assert.deepEqual(plain(a['Daily activity'].at(-1)),['2026-10-28',1,0,1]);
 assert.deepEqual(plain(a['Daily activity'].at(-3)),['2026-10-26',1,1,0]);
 assert.equal(a['Installation history'].length,4);
});
test('return metrics exclude immature windows, unknown first seen, and expired history',()=>{
 const {sandbox:s}=fixture(),today=s.dayNumber(now);let rows=[];
 for(const [id,day] of [['too-old',-100],['returned',-40],['not-returned',-40],['too-old',-40],['returned',-30],['immature',-13],['returned',-8],['immature',0],['too-old',0]]) rows=observe(s,rows,id,day);
 const a=s.analytics_(rows,today);
 assert.equal(metric(a,'Returned in week 2'),0.5);assert.equal(metric(a,'Returned in week 5'),0.5);
 assert.match(a.Dashboard.find(r=>r[0]==='Returned in week 2')[2],/^1 of 2 /);
});
test('version history respects opt-out, stale revisions, status-only and expiry',()=>{
 const {sandbox:s}=fixture(),today=s.dayNumber(now);let rows=observe(s,[],'one',0);
 const off={kind:'status',revision:1,enabled:false,changed_at:+now/1000};
 rows=s.applyReport(rows,'one',off,now);const history=rows[0][15];
 rows=observe(s,rows,'one',1,'0.5.0');assert.equal(rows[0][15],history);
 rows=s.applyReport(rows,'one',{...off,revision:2,enabled:true},date(1));
 rows=observe(s,rows,'one',1,'0.5.0');assert.equal(rows[0][15],history);
 assert.equal(s.currentRows(rows,today+91).length,0);
});
test('new history column rejects occupied cells, formulas and malformed events before writes',()=>{
 for(const [value,formula] of [[42,''],['','=""'],['[[46000,"0.4.1"],[46000,"0.4.2"]]',''],['[[46000,"=IMPORTXML()"]]','']]) {
  const f=fixture();f.raw.maxColumns=26;f.raw.maxRows=100;f.raw.put(60,16,value,formula);
  const before=f.snapshot();assert.throws(()=>f.sandbox.setup());assert.equal(f.snapshot(),before);
 }
});
test('analytics tab collisions and header/formula edits refuse before any source write',()=>{
 for(const name of ['Dashboard','Versions','Daily activity','Installation history']) for(const operation of ['setup','maintenance','post']) {
  const f=fixture();if(operation!=='setup') f.sandbox.setup();
  const sheet=f.add(name);sheet.put(1,1,'Owner data');const before=f.snapshot();
  if(operation==='post') assert.equal(f.sandbox.doPost({postData:{contents:JSON.stringify(report)}}).ok,false);
  else assert.throws(()=>f.sandbox[operation]());
  assert.equal(f.snapshot(),before);
 }
 const f=fixture();f.sandbox.setup();f.sheets.get('Versions').put(10,2,'','=""');
 const before=f.snapshot();assert.throws(()=>f.sandbox.maintenance());assert.equal(f.snapshot(),before);
});
test('analytics recover interrupted creation and clear expired derived records',()=>{
 for(const cell of [1,2,5,12,25]) {
  const f=fixture();f.sandbox.setup();const sheet=f.sheets.get('Dashboard');sheet.cells.clear();
  f.failAfterCells(cell);assert.throws(()=>f.sandbox.maintenance());f.sandbox.maintenance();
  assert.equal(sheet.cell(4,1).value,'Metric');
 }
 const f=fixture(),s=f.sandbox,today=s.dayNumber(new Date());s.setup();
 f.seed([['expired',today-90,'0.4.0','','enabled','','',0,'','','','','','','','']]);s.maintenance();
 assert.equal(f.rows().length,1);assert.equal(f.sheets.get('Installation history').getLastRow(),4);
});
test('ordinary report updates one derived installation row, not the entire table',()=>{
 const f=fixture(),s=f.sandbox;let rows=[];
 for(let i=0;i<600;i++) rows=observe(s,rows,String(i),0);
 const today=s.dayNumber(now);s.setup();
 s.analyticsWrite_({getSheetByName:n=>f.sheets.get(n)},rows,today,s.analyticsPreflight_({getSheetByName:n=>f.sheets.get(n)}));
 const sheet=f.sheets.get('Installation history'),calls=[],range=sheet.getRange.bind(sheet);
 sheet.getRange=(...args)=>{const r=range(...args),write=r.setValues;r.setValues=v=>{calls.push(args);write(v);};return r;};
 rows=observe(s,rows,'300',0,'0.4.1');
 s.analyticsWrite_({getSheetByName:n=>f.sheets.get(n)},rows,today,s.analyticsPreflight_({getSheetByName:n=>f.sheets.get(n)}));
 assert.equal(calls.length,1);assert.equal(calls[0][2],1);
});
test('native date coercion does not rewrite unchanged analytics and version categories stay bounded',()=>{
 const f=fixture(),s=f.sandbox,today=s.dayNumber(now);let rows=[];
 for(let i=0;i<30;i++) rows=observe(s,rows,String(i),0,`0.4.${i}`);
 const book={getSheetByName:n=>f.sheets.get(n)};s.setup();
 s.analyticsWrite_(book,rows,today,s.analyticsPreflight_(book));
 const sheet=f.sheets.get('Daily activity');
 for(let r=5;r<=94;r++) sheet.put(r,1,new Date(sheet.cell(r,1).value+'T00:00:00.000Z'));
 const before=f.state.writes;s.analyticsWrite_(book,rows,today,s.analyticsPreflight_(book));
 assert.equal(f.state.writes,before);
 const versions=s.analytics_(rows,today).Versions;
 assert.equal(versions.length,21);assert.equal(versions.at(-1)[0],'Other versions');
 assert.equal(versions.reduce((n,r)=>n+r[2],0),30);
});
