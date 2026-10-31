const test=require('node:test'),assert=require('node:assert/strict');
const {fixture}=require('./collector.fixture.cjs');
const base={schema:5,kind:'activity',installation:'a'.repeat(64),revision:0,version:'0.5.0',country:null,first_started_at:null,
 package_format:'deb',update_channel:'apt',installed_app_version:'0.5.0'};
const now=new Date('2026-10-30T12:00:00Z'),date=n=>new Date(+now+n*86400000);
const plain=v=>JSON.parse(JSON.stringify(v));

test('schema 5 is bounded and rejects impossible or identifying channel fields',()=>{
 const s=fixture().sandbox;
 for(const [package_format,update_channel,installed_app_version] of [['deb','apt','0.5.0'],['rpm','dnf','0.5.0'],['deb','disabled','0.5.0'],['rpm','manual',null],['source','source',null],['unknown','unknown',null]])
  assert.equal(s.validateReport(JSON.stringify({...base,package_format,update_channel,installed_app_version})).schema,5);
 for(const change of [{package_format:'rpm',update_channel:'apt'},{package_format:'unknown'}, {package_format:'source',update_channel:'source'},
   {update_channel:'https://example.test/user/repo'}, {installed_app_version:'0:0.5.0-1.fc43'}, {hostname:'private'}, {package_format:null}])
  assert.throws(()=>s.validateReport(JSON.stringify({...base,...change})));
});
test('daily channel filtering never projects a new package channel onto old reports',()=>{
 const s=fixture().sandbox;let rows=s.applyReport([],'one',{...base,schema:4,package_format:undefined,update_channel:undefined,installed_app_version:undefined},date(-2));
 rows=s.applyReport(rows,'one',base,date(-1));
 rows=s.applyReport(rows,'one',{...base,package_format:'rpm',update_channel:'dnf'},now);
 const a=s.analytics_(rows,s.dayNumber(now));
 assert.deepEqual(plain(a['Channel activity'].map(r=>r.slice(0,4))),[
  ['2026-10-28','unknown','unknown',1],['2026-10-29','deb','apt',1],['2026-10-30','rpm','dnf',1]]);
 assert.equal(a['Installation channels'][0][1],'rpm');
});
test('installed version is distinct from observed running-version adoption',()=>{
 const s=fixture().sandbox;let rows=s.applyReport([],'one',{...base,version:'0.4.1'},date(-1));
 let a=s.analytics_(rows,s.dayNumber(now));
 assert.equal(a['Version changes'].length,0);
 assert.equal(a['Installation channels'][0][3],'0.5.0');
 assert.equal(a.Dashboard.find(r=>r[0]==='Different installed version / 30 days')[1],1);
 rows=s.applyReport(rows,'one',base,now);a=s.analytics_(rows,s.dayNumber(now));
 assert.deepEqual(plain(a['Version changes']),[['0.4.1','0.5.0','deb','apt','2026-10-30',1,1]]);
});
test('missing history and interrupted prefixes stay unknown and are recoverable',()=>{
 const f=fixture(),s=f.sandbox,today=s.dayNumber(now);s.setup();
 let rows=s.applyReport([],'one',base,date(-1));
 rows[0][1]=today;rows[0][14]=now.toISOString();
 assert.equal(s.latestObservation_(rows[0])[1],'unknown');
 assert.equal(s.analytics_(rows,today)['Channel versions'][0][0],'unknown');
 rows=s.applyReport(rows,'one',{...base,package_format:'rpm',update_channel:'dnf'},now);
 assert.equal(s.latestObservation_(rows[0])[1],'rpm');
 f.seed(rows);s.setup();s.setup();assert.equal(s.statusPlan_(f.raw,true).rows[0][16],rows[0][16]);
});
test('duplicate rows, daily retries, opt-out and expiration do not inflate channel counts',()=>{
 const s=fixture().sandbox,today=s.dayNumber(now);let rows=s.applyReport([],'one',base,now);
 rows=s.applyReport([...rows,...rows],'one',base,now);
 assert.equal(s.observations_(rows[0][16]).length,1);
 assert.equal(s.analytics_(rows,today)['Channel activity'][0][3],1);
 const saved=rows[0][16];
 rows=s.applyReport(rows,'one',{kind:'status',revision:1,enabled:false,changed_at:+now/1000},now);
 rows=s.applyReport(rows,'one',{...base,version:'0.6.0'},date(1));
 assert.equal(rows[0][16],saved);assert.equal(s.analytics_(rows,today)['Channel versions'].length,0);
 assert.equal(s.currentRows(rows,today+90).length,0);
});
test('schema migration refuses a preexisting owner formula in the new source column',()=>{
 const f=fixture(),s=f.sandbox;f.raw.maxColumns=26;f.raw.maxRows=10;
 f.raw.put(2,17,'','=PRIVATE_NOTE()');const before=f.snapshot();
 assert.throws(()=>s.setup());assert.equal(f.snapshot(),before);
});
test('same-day channel transitions retain the last actual observation with a 90-day bound',()=>{
 const s=fixture().sandbox;let rows=[];
 for(let i=0;i<100;i++) rows=s.applyReport(rows,'one',base,date(i));
 assert.equal(s.observations_(rows[0][16]).length,90);
 rows=s.applyReport(rows,'one',{...base,package_format:'rpm',update_channel:'dnf'},new Date(+date(99)+1000));
 const entries=s.observations_(rows[0][16]);
 assert.equal(entries.length,90);assert.equal(entries.at(-1)[1],'rpm');
});
