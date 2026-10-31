const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const context = {module:{exports:{}}};
vm.runInNewContext(fs.readFileSync(__dirname+'/collector.gs','utf8'),context);
const {validateReport,updateRows}=context.module.exports;
const payload={schema:1,installation:'a'.repeat(64),version:'0.2.0'};
test('accepts only the small documented report',()=>{
 assert.equal(validateReport(JSON.stringify(payload)).version,'0.2.0');
 for (const invalid of [{...payload,email:'private@example.com'},{...payload,schema:2},{...payload,version:'=IMPORTXML("url")'},{...payload,installation:'short'},null,[]])
  assert.throws(()=>validateReport(JSON.stringify(invalid)));
 assert.throws(()=>validateReport(' '.repeat(1025)));
});
test('repeat reports keep one row and update the version',()=>{
 const rows=updateRows([], 'id-a','0.2.0',46000);
 assert.equal(rows.length,1);
 const twice=updateRows(rows,'id-a','0.1.3',46000);
 assert.equal(twice.length,1); assert.equal(twice[0][2],'0.1.3');
 const next=updateRows(twice,'id-a','0.1.3',46001);
 assert.equal(next[0][1],46001); assert.equal(next.length,1);
});
test('different installations remain distinct and inactive records expire',()=>{
 const rows=[['expired',45910,'0.2.0'],['retained',45911,'0.2.0']];
 const next=updateRows(rows,'new','0.2.0',46000);
 assert.equal(next.length,2); assert.equal(next[0][0],'retained');
 assert.equal(rows.length,2); assert.equal(rows[0][0],'expired');
});
test('existing identities can report at capacity, new ones cannot',()=>{
 const rows=Array.from({length:20000},(_,i)=>['id-'+i,46000,'0.2.0']);
 assert.equal(updateRows(rows,'id-0','0.1.3',46000).length,20000);
 assert.throws(()=>updateRows(rows,'new','0.2.0',46000));
});

const {fixture}=require('./collector.fixture.cjs');
test('setup prepares imported small sheets and creates only one retention trigger',()=>{
 const f=fixture();
 assert.equal(f.sandbox.setup().configured,true);
 assert.equal(f.sandbox.setup().configured,true);
 assert.equal(f.status(),'Configured'); assert.equal(f.triggers().length,1);
});
test('real request handler stores a hash, date, version and optional country and deduplicates reports',()=>{
 const f=fixture();f.sandbox.setup();
 const request={postData:{contents:JSON.stringify(payload)}};
 const ack=f.sandbox.doPost(request);
 assert.equal(ack.ok,true);assert.equal(ack.day,new Date().toISOString().slice(0,10));
 assert.equal(f.sandbox.doPost(request).ok,true);
 assert.equal(f.rows().length,2); assert.equal(f.rows()[1].length,16);
 assert.match(f.rows()[1][0],/^[a-f0-9]{64}$/);assert.notEqual(f.rows()[1][0],payload.installation);
 assert.equal(f.rows()[1][2],payload.version);assert.equal(f.locked(),false);
 assert.equal(f.sandbox.doPost({postData:{contents:JSON.stringify({...payload,email:'private@example.com'})}}).ok,false);
 assert.equal(f.rows().length,2);
 assert.deepEqual(f.sandbox.doGet(),{service:'halcyon-activity',schema:5});
});

test('retry after an interrupted pruning write counts each installation once',()=>{
 const f=fixture();f.sandbox.setup();
 const today=Math.floor(Date.now()/86400000)+25569;
 f.seed([['e'.repeat(64),today-100,'0.2.0'],['f'.repeat(64),today-100,'0.2.0'],
         ['a'.repeat(64),today,'0.2.0'],['b'.repeat(64),today,'0.2.0']]);
 f.failAfterWrite();
 const request={postData:{contents:JSON.stringify(payload)}};
 assert.equal(f.sandbox.doPost(request).ok,false);
 assert.equal(f.sandbox.doPost(request).ok,true);
 assert.equal(f.rows().length,4); // header plus three distinct installations
 assert.equal(new Set(f.rows().slice(1).map(row=>row[0])).size,3);
});
test('maintenance repairs duplicate records and preserves the newest date and version',()=>{
 const f=fixture();f.sandbox.setup();
 const today=Math.floor(Date.now()/86400000)+25569;
 f.seed([['a',today-3,'0.1.1'],['a',today-1,'0.2.0'],['b',today-5,'0.1.1'],['expired',today-90,'0.1.1']]);
 f.sandbox.maintenance();
 assert.equal(f.rows().length,3);
 assert.equal(f.rows()[1][1],today-1);assert.equal(f.rows()[1][2],'0.2.0');
});

test('country reports accept schema 2 and reject unbounded or extra location data',()=>{
 const report={...payload,schema:2,country:'HR'};
 assert.equal(validateReport(JSON.stringify(report)).country,'HR');
 assert.equal(validateReport(JSON.stringify({...report,country:null})).country,null);
 for(const country of ['hr','H','HRV','1A','=US',{},42])
  assert.throws(()=>validateReport(JSON.stringify({...report,country})));
 assert.throws(()=>validateReport(JSON.stringify({...report,ip:'192.0.2.1'})));
});
test('first country survives later reports and old clients cannot erase it',()=>{
 let rows=updateRows([], 'country-id','0.2.0',46000,'HR');
 assert.equal(rows[0][3],'HR');
 rows=updateRows(rows,'country-id','0.2.0',46001,'US');
 assert.equal(rows[0][3],'HR');
 assert.equal(updateRows(rows,'country-id','0.2.0',46002)[0][3],'HR');
 assert.equal(updateRows([['unknown',46000,'0.2.0']], 'unknown','0.2.0',46001,'DE')[0][3],'DE');
});
test('country totals count distinct active installations in UTC windows, including Unknown',()=>{
 assert.equal(typeof context.countryCounts,'function');
 const rows=[['a',46000,'0.2.0','HR'],['a',45999,'0.1.1','HR'],['b',45994,'0.2.0','HR'],
 ['c',45971,'0.2.0','US'],['d',46000,'0.2.0',''],['old',45970,'0.1.1','DE']];
 assert.equal(JSON.stringify(context.countryCounts(rows,46000)),JSON.stringify([
 ['HR',1,2,2],['Unknown',1,1,1],['US',0,0,1]]));
});

test('request handlers migrate old rows and refresh country counts after retention',()=>{
 const f=fixture();f.sandbox.setup();
 const today=Math.floor(Date.now()/86400000)+25569;
 f.seed([['legacy',today,'0.1.1'],['expired',today-90,'0.2.0','US']]);
 const request={postData:{contents:JSON.stringify({...payload,schema:2,country:'HR'})}};
 assert.equal(f.sandbox.doPost(request).ok,true);
 assert.equal(f.rows()[1][3],'');
 assert.equal(f.rows()[2][3],'HR');
 assert.equal(JSON.stringify(f.countries()),JSON.stringify([
 ['First reported country','Active '+new Date().toISOString().slice(0,10)+' (UTC)','Last 7 days','Last 30 days'],['HR',1,1,1],['Unknown',1,1,1]]));
 f.seed([['expired',today-90,'0.2.0','US']]);f.sandbox.maintenance();
 assert.ok(f.countries().slice(1).every(row=>row.every(value=>value==='')));
});

test('schema 3 accepts minimal choices and ordered activity only',()=>{
 const status={schema:3,kind:'status',installation:payload.installation,revision:1,enabled:false,changed_at:0};
 assert.equal(validateReport(JSON.stringify(status)).enabled,false);
 for (const bad of [{...status,revision:0},{...status,revision:1.5},{...status,revision:2**53},
  {...status,enabled:'false'},{...status,changed_at:-1},{...status,changed_at:Infinity},
  {...status,country:'HR'},{...status,version:'0.2.1'},{...status,changed_at:253402300800}])
  assert.throws(()=>validateReport(JSON.stringify(bad)));
 assert.equal(validateReport(JSON.stringify({...payload,schema:3,kind:'activity',revision:0,country:null})).kind,'activity');
});
test('optout is idempotent, inactive, excludes countries, and rejects old or reordered activity',()=>{
 const now=new Date('2026-10-23T12:00:00Z'),today=context.dayNumber(now);
 const off={schema:3,kind:'status',revision:1,enabled:false,changed_at:123};
 let rows=context.applyReport([], 'id', {schema:2,version:'0.2.0',country:'HR'}, now);
 rows=context.applyReport(rows,'id',off,now);
 assert.equal(rows[0][1],today);assert.equal(rows[0][4],'opted_out');assert.equal(rows[0][5],new Date(123000).toISOString());
 assert.equal(context.countryCounts(rows,today).length,0);
 for(const report of [off,{schema:1,version:'0.2.0'},{schema:2,version:'0.2.0',country:'US'},
  {schema:3,kind:'activity',revision:0,version:'0.2.1',country:'US'},
  {schema:3,kind:'activity',revision:2,version:'0.2.1',country:null}])
  assert.equal(JSON.stringify(context.applyReport(rows,'id',report,new Date(+now+86400000))),JSON.stringify(rows));
 const on={...off,revision:2,enabled:true,changed_at:5}; // skew does not order choices
 const enabled=context.applyReport(rows,'id',on,new Date(+now+86400000));
 assert.equal(enabled[0][1],today);assert.equal(enabled[0][5],rows[0][5]);
 assert.equal(JSON.stringify(context.applyReport(enabled,'id',off,now)),JSON.stringify(enabled));
 assert.equal(context.applyReport(enabled,'id',{schema:3,kind:'activity',revision:1,version:'0.2.1'},now)[0][1],today);
 assert.equal(context.applyReport(enabled,'id',{schema:3,kind:'activity',revision:2,version:'0.2.1'},new Date(+now+86400000))[0][1],today+1);
});
test('first-run optout has no activity, version or country, expires from receipt, and dedup keeps strongest status',()=>{
 const now=new Date('2026-10-23T12:00:00Z'),today=context.dayNumber(now);
 const off={schema:3,kind:'status',revision:3,enabled:false,changed_at:253402300799};
 const rows=context.applyReport([], 'id', off, now);
 assert.equal(rows[0][1],'');assert.equal(rows[0][2],'');assert.equal(rows[0][3],'');
 assert.equal(context.currentRows(rows,today+89).length,1);assert.equal(context.currentRows(rows,today+90).length,0);
 const old=['id',today+1,'0.2.0','HR','enabled','','',0,''];
 for(const duplicates of [[...rows,old],[old,...rows]]) {
  const fixed=context.currentRows(duplicates,today);
  assert.equal(fixed.length,1);assert.equal(fixed[0][4],'opted_out');assert.equal(fixed[0][1],'');
 }
});
test('migration is additive, idempotent, preserves old rows and custom summary formulas even after interruption',()=>{
 const f=fixture(),today=context.dayNumber(new Date());
 const original=[['old',today-120,'0.1.1','HR'],['new',today,'0.2.0','US']];
 f.seed(original);f.failAfterWrite();assert.throws(()=>f.sandbox.setup());
 assert.equal(f.sandbox.setup().configured,true);assert.equal(f.sandbox.setup().configured,true);
 assert.equal(JSON.stringify(f.rows().slice(1).map(r=>r.slice(0,4))),JSON.stringify(original));
 assert.equal(f.rows()[1][4],'enabled');assert.equal(f.rows()[1][7],0);
 assert.match(f.summary()[0][0],/Installations!E2:E20001,"enabled"/);
 assert.equal(f.summary()[1][0],'=SUM(Historical!B2:B30)');
 assert.equal(f.triggers().length,1);
});
test('handler interrupted after status write retries without extending receipt or counting activity',()=>{
 const f=fixture();f.sandbox.setup();
 const request={postData:{contents:JSON.stringify({schema:3,kind:'status',installation:payload.installation,revision:1,enabled:false,changed_at:0})}};
 f.failAfterWrite();assert.equal(f.sandbox.doPost(request).ok,false);
 const receipt=f.rows()[1][6];
 assert.equal(f.sandbox.doPost(request).ok,true);assert.equal(f.rows()[1][6],receipt);
 assert.equal(f.rows()[1][1],'');assert.equal(f.rows()[1][2],'');assert.equal(f.rows()[1][3],'');
 assert.equal(f.reporting()[1][1],0);assert.equal(f.reporting()[4][1],1);
 assert.match(f.reporting()[4][0],/90 days/);
 assert.equal(f.sandbox.doPost({postData:{contents:JSON.stringify(payload)}}).ok,true);
 assert.equal(f.rows()[1][1],'');assert.equal(f.reporting()[1][1],0);
});
