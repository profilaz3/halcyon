'use strict';
// Update only the four explicitly owned reader surfaces. Collector tables and
// sharing are never rewritten by this presentation migration.
const OWNER='halcyon:presentation:v2', PREVIOUS='halcyon:presentation:v1';
function build(metadata,inspected) {
 const by=Object.fromEntries(metadata.sheets.map(s=>[s.properties.title,s])), q=[];
 const readers=['Overview','Activity','Versions','History'];
 for(const name of readers) {
  const sheet=inspected.sheets.find(s=>s.properties.title===name);
  let marker;
  for(const d of sheet?.data||[]) for(const [i,row] of (d.rowData||[]).entries())
   for(const [j,cell] of (row.values||[]).entries()) if((d.startRow||0)+i===0&&(d.startColumn||0)+j===25)marker=cell.userEnteredValue?.stringValue;
  if(![PREVIOUS,OWNER].includes(marker))throw Error('Unowned reader: '+name);
 }
 const id=n=>by[n].properties.sheetId;
 const range=(n,r,e,c=0,d=12)=>({sheetId:id(n),startRowIndex:r,endRowIndex:e,startColumnIndex:c,endColumnIndex:d});
 const rgb=h=>({red:parseInt(h.slice(0,2),16)/255,green:parseInt(h.slice(2,4),16)/255,blue:parseInt(h.slice(4,6),16)/255});
 const C={ink:rgb('17343B'),teal:rgb('087F8C'),muted:rgb('60717A'),light:rgb('EDF6F7'),blue:rgb('7999B2'),white:rgb('FFFFFF'),line:rgb('E2E9EC')};
 const cell=(n,r,c,v,note)=>q.push({updateCells:{range:range(n,r,r+1,c,c+1),rows:[{values:[{userEnteredValue:typeof v==='number'?{numberValue:v}:v.startsWith('=')?{formulaValue:v}:{stringValue:v},...(note?{note}:{})}]}],fields:'userEnteredValue'+(note?',note':'')}});
 const fmt=(n,r,e,c,d,f)=>q.push({repeatCell:{range:range(n,r,e,c,d),cell:{userEnteredFormat:f},fields:Object.keys(f).map(k=>'userEnteredFormat.'+k).join(',')}});
 const dim=(n,dimension,startIndex,endIndex,properties)=>q.push({updateDimensionProperties:{range:{sheetId:id(n),dimension,startIndex,endIndex},properties,fields:Object.keys(properties).join(',')}});
 const text=(n,r,c,d,v,note)=>{cell(n,r,c,v,note);if(d>c+1)q.push({mergeCells:{range:range(n,r,r+1,c,d),mergeType:'MERGE_ALL'}});};
 const font=(size=11,bold=false,color=C.ink)=>({fontFamily:'Arial',fontSize:size,bold,foregroundColorStyle:{rgbColor:color}});
 const metric=k=>'=IFNA(VLOOKUP("'+k+'",Dashboard!A5:B30,2,FALSE),"Unknown")';
 const header=(n,r,c,labels)=>{labels.forEach((v,j)=>cell(n,r,c+j,v));fmt(n,r,r+1,c,c+labels.length,{backgroundColorStyle:{rgbColor:C.light},textFormat:font(10,true)});dim(n,'ROWS',r,r+1,{pixelSize:36});};
 const control=(n,r,c,d,label,choices)=>{text(n,r-1,c,d,label);text(n,r,c,d,choices[0]);fmt(n,r,r+1,c,d,{backgroundColorStyle:{rgbColor:C.light},textFormat:font(11,true,C.teal)});q.push({setDataValidation:{range:range(n,r,r+1,c,c+1),rule:{condition:{type:'ONE_OF_LIST',values:choices.map(userEnteredValue=>({userEnteredValue}))},strict:true,showCustomUi:true}}});};
 const section=(n,r,c,d,label)=>{text(n,r,c,d,label);fmt(n,r,r+1,c,d,{textFormat:font(13,true)});dim(n,'ROWS',r,r+1,{pixelSize:34});};
 function chart(cid,n,r,c,width,height,title,source,r0,r1,domain,series,type='LINE') {
  const src=col=>({sourceRange:{sources:[range(source,r0,r1,col,col+1)]}});
  q.push({addChart:{chart:{chartId:cid,spec:{title,fontName:'Arial',titleTextFormat:{fontSize:14,bold:true,foregroundColorStyle:{rgbColor:C.ink}},backgroundColorStyle:{rgbColor:C.white},hiddenDimensionStrategy:'SHOW_ALL',basicChart:{chartType:type,headerCount:1,legendPosition:series.length>1?'BOTTOM_LEGEND':'NO_LEGEND',domains:[{domain:src(domain)}],series:series.map((s,i)=>({series:src(s),targetAxis:type==='BAR'?'BOTTOM_AXIS':'LEFT_AXIS',colorStyle:{rgbColor:i?C.blue:C.teal},...(type==='BAR'?{dataLabel:{type:'DATA'}}:{})})),axis:[{position:'BOTTOM_AXIS',...(type==='BAR'?{viewWindowOptions:{viewWindowMin:0}}:{})},{position:'LEFT_AXIS',...(type==='BAR'?{}:{viewWindowOptions:{viewWindowMin:0}})}]}},position:{overlayPosition:{anchorCell:{sheetId:id(n),rowIndex:r,columnIndex:c},widthPixels:width,heightPixels:height}}}}});
 }
 for(const n of readers) {
  const offset=n==='Versions'?4:0,end=n==='History'?20020:n==='Overview'?280:135,s=by[n];
  // All these regions were reserved by v1. Foreign drawings are preserved.
  for(const c of s.charts||[])if(c.chartId>=804050100&&c.chartId<804050200)q.push({deleteEmbeddedObject:{objectId:c.chartId}});
  for(const merge of s.merges||[])if((merge.startColumnIndex||0)>=offset)q.push({unmergeCells:{range:merge}});
  for(const g of s.rowGroups||[])q.push({deleteDimensionGroup:{range:g.range}});
  for(let i=(s.conditionalFormats||[]).length-1;i>=0;i--)q.push({deleteConditionalFormatRule:{sheetId:id(n),index:i}});
  q.push({clearBasicFilter:{sheetId:id(n)}});
  q.push({updateCells:{range:range(n,0,end,offset,26),fields:'userEnteredValue,userEnteredFormat,note,dataValidation'}});
  dim(n,'ROWS',0,end,{hiddenByUser:false,pixelSize:n==='History'?30:27});
  dim(n,'COLUMNS',offset,26,{hiddenByUser:false,pixelSize:80});
  dim(n,'COLUMNS',offset+12,26,{hiddenByUser:true});
  fmt(n,0,n==='History'?20020:135,offset,offset+12,{backgroundColorStyle:{rgbColor:C.white},textFormat:font(),verticalAlignment:'MIDDLE',wrapStrategy:'WRAP',numberFormat:{type:'NUMBER',pattern:'#,##0'}});
  fmt(n,0,3,offset,offset+12,{backgroundColorStyle:{rgbColor:C.ink},textFormat:font(11,false,C.white)});
  text(n,0,offset,offset+12,n==='Overview'?'Halcyon activity':n==='Activity'?'Activity over time':n==='Versions'?'Versions & updates':'Installation history');
  fmt(n,0,1,offset,offset+12,{textFormat:font(24,true,C.white)});dim(n,'ROWS',0,1,{pixelSize:48});
  text(n,1,offset,offset+12,'=IF(ISNUMBER(Reporting!B1),"Updated "&TEXT(Reporting!B1,"d mmm yyyy")&" UTC. Counts are installations.","Awaiting collector snapshot")');
  dim(n,'ROWS',2,3,{pixelSize:12});dim(n,'ROWS',3,4,{pixelSize:18});
  cell(n,0,25,OWNER);
  q.push({updateSheetProperties:{properties:{sheetId:id(n),hidden:false,index:readers.indexOf(n),tabColorStyle:{rgbColor:n==='Overview'?C.teal:C.ink},gridProperties:{frozenRowCount:n==='History'?7:3,hideGridlines:true,frozenColumnCount:0}},fields:'hidden,index,tabColorStyle,gridProperties.frozenRowCount,gridProperties.hideGridlines,gridProperties.frozenColumnCount'}});
 }
 // Headline cards. Full definitions stay in hover notes, not dense paragraphs.
 const cards=[['Active in 30 days','Active in 30 days','Distinct reporting installations'],['Active in 7 days','Active in 7 days','Reported this week'],['Active today','Active today','Today is still in progress'],['Used on 2+ days','Used on 2+ days this week','Within the past 7 days']];
 cards.forEach(([label,key,hint],i)=>{const c=i*3;fmt('Overview',4,8,c,c+3,{backgroundColorStyle:{rgbColor:C.light}});text('Overview',4,c,c+3,label,'Enabled installations with a received report in this window. Offline activity is not reconstructed.');text('Overview',5,c,c+3,metric(key));fmt('Overview',5,6,c,c+3,{textFormat:font(30,true,C.teal),horizontalAlignment:'LEFT'});dim('Overview','ROWS',5,6,{pixelSize:48});text('Overview',7,c,c+3,hint);fmt('Overview',7,8,c,c+3,{textFormat:font(9,false,C.muted)});});
 // Charts use bounded collector-owned sources, never copies of installation data.
 cell('Overview',0,13,'Date');cell('Overview',0,14,'Active');
 cell('Overview',1,13,"=IFNA(FILTER('Daily activity'!A5:B94,ISNUMBER('Daily activity'!B5:B94)),\"\")");
 chart(804050101,'Overview',9,0,620,318,'Daily reports','Overview',0,91,13,[14]);
 chart(804050102,'Overview',9,8,320,318,'Versions in use · 30 days','Versions',3,25,0,[2],'BAR');
 section('Overview',22,0,6,'Update channels');section('Overview',22,7,12,'Data coverage');
 header('Overview',23,0,['Package','Channel','Active / 30d']);
 const channels=[['DEB','APT','deb','apt'],['RPM','DNF','rpm','dnf'],['Any','Manual','*','manual'],['Any','Disabled','*','disabled'],['Source','Source','source','source'],['Unknown','Unknown','unknown','unknown']];
 channels.forEach(([label,channel,format,key],i)=>{cell('Overview',24+i,0,label);cell('Overview',24+i,1,channel);cell('Overview',24+i,2,`=SUMIFS('Channel versions'!E5:E205,'Channel versions'!A5:A205,"${format}",'Channel versions'!B5:B205,"${key}")`);});
 cell('Overview',29,0,'Any');cell('Overview',29,2,'=Dashboard!B7-SUM(C25:C29)');
 text('Overview',24,7,12,'=Dashboard!B17&" of "&Dashboard!B7&" active installations report a package type."');
 text('Overview',26,7,12,'Older clients appear as Unknown. Channel details arrive with the next app update.');dim('Overview','ROWS',26,27,{pixelSize:48});
 text('Overview',29,7,12,'=IFNA("Latest activity received: "&SUBSTITUTE(LEFT(INDEX(SORT(FILTER(Installations!O2:O20001,Installations!O2:O20001<>""),1,FALSE),1),16),"T"," ")&" UTC","No activity receipt")');
 section('Overview',32,0,6,'Countries · active in 30 days');header('Overview',33,0,['Country','Active / 30d']);
 cell('Overview',34,0,'=IFNA(FILTER({Countries!A2:A251,Countries!D2:D251},Countries!A2:A251<>""),"No country reports")');
 text('Overview',32,7,12,'Retained history: up to 90 days.');text('Overview',34,7,12,'Opt-outs and expiry can reduce historical counts. First seen means first report, not installation.');dim('Overview','ROWS',34,35,{pixelSize:48});
 // Activity: channel filter follows observations on each individual day.
 control('Activity',5,0,2,'Window',['Past 7 days','Past 30 days','Past 90 days']);
 control('Activity',5,3,5,'Package',['All packages','DEB','RPM','Source','Unknown']);
 control('Activity',5,6,8,'Channel',['All channels','APT','DNF','Manual','Disabled','Source','Unknown']);
 chart(804050103,'Activity',7,0,960,324,'Reported activity','Activity',20,111,0,[2,3],'COLUMN');
 header('Activity',20,0,['Date · UTC','Active','First report','Reported before']);
 const activityFilter="'Channel activity'!A5:F5000";
 // The existing reader uses FILTER for live row selection. Preserve that
 // dynamic view; raw source data and original dates stay untouched.
 cell('Activity',21,0,`=IFNA(QUERY(${activityFilter},"select A,sum(D),sum(E),sum(F) where A >= '"&TEXT(Reporting!B1-VALUE(REGEXEXTRACT(A6,"[0-9]+"))+1,"yyyy-mm-dd")&"'"&IF(D6="All packages",""," and B = '"&LOWER(D6)&"'")&IF(G6="All channels",""," and C = '"&LOWER(G6)&"'")&" group by A order by A label A '',sum(D) '',sum(E) '',sum(F) ''",0),"No reports in this window")`);
 dim('Activity','COLUMNS',0,1,{pixelSize:145});dim('Activity','COLUMNS',1,4,{pixelSize:105});dim('Activity','COLUMNS',4,12,{pixelSize:65});
 section('Activity',21,5,12,'Return visits');
 text('Activity',22,5,12,'Second week');text('Activity',23,5,12,metric('Returned in week 2'));fmt('Activity',23,24,5,12,{numberFormat:{type:'PERCENT',pattern:'0%'},textFormat:font(18,true,C.teal)});
 text('Activity',25,5,12,'=VLOOKUP("Returned in week 2",Dashboard!A5:C30,3,FALSE)');dim('Activity','ROWS',25,26,{pixelSize:64});
 text('Activity',27,5,12,'Fifth week');text('Activity',28,5,12,metric('Returned in week 5'));fmt('Activity',28,29,5,12,{numberFormat:{type:'PERCENT',pattern:'0%'},textFormat:font(18,true,C.teal)});
 text('Activity',30,5,12,'=VLOOKUP("Returned in week 5",Dashboard!A5:C30,3,FALSE)');dim('Activity','ROWS',30,31,{pixelSize:64});
 text('Activity',34,5,12,'Return visits use all retained, enabled installations with a complete observation window. They are not filtered by channel.');dim('Activity','ROWS',34,35,{pixelSize:68});
 // Versions: one adoption table plus actual observed transitions.
 control('Versions',5,4,6,'Window',['Past 30 days','Past 7 days']);
 control('Versions',5,7,9,'Package',['All packages','DEB','RPM','Source','Unknown']);
 control('Versions',5,10,12,'Channel',['All channels','APT','DNF','Manual','Disabled','Source','Unknown']);
 header('Versions',7,4,['Version','Active']);
 cell('Versions',8,4,`=IFNA(QUERY('Channel versions'!A5:E205,"select C,sum("&IF(E6="Past 7 days","D","E")&") where C is not null"&IF(H6="All packages",""," and A = '"&LOWER(H6)&"'")&IF(K6="All channels",""," and B = '"&LOWER(K6)&"'")&" group by C order by sum("&IF(E6="Past 7 days","D","E")&") desc label C '',sum("&IF(E6="Past 7 days","D","E")&") ''",0),"No reports")`);
 chart(804050104,'Versions',7,8,640,324,'Versions in use','Versions',7,29,4,[5],'BAR');
 section('Versions',22,4,16,'Observed version changes');
 text('Versions',23,4,16,'A change means the installation later reported another running version. It does not prove an update succeeded.');dim('Versions','ROWS',23,24,{pixelSize:44});
 header('Versions',25,4,['From','To','Installations']);
 cell('Versions',26,4,`=IFNA(QUERY('Version changes'!A5:G205,"select A,B,sum("&IF(E6="Past 7 days","F","G")&") where A is not null"&IF(H6="All packages",""," and C = '"&LOWER(H6)&"'")&IF(K6="All channels",""," and D = '"&LOWER(K6)&"'")&" group by A,B label A '',B '',sum("&IF(E6="Past 7 days","F","G")&") ''",0),"No version changes observed")`);
 text('Versions',25,9,16,'Installed vs running');text('Versions',27,9,16,'=IF(Dashboard!B17=0,"No installed-version reports yet.",Dashboard!B19&" active installations reported a different installed version.")');dim('Versions','ROWS',27,28,{pixelSize:50});
 text('Versions',30,9,16,'The app may still be running after a package update. Older clients do not supply the installed version.');dim('Versions','ROWS',30,31,{pixelSize:55});
 dim('Versions','COLUMNS',0,4,{hiddenByUser:true});
 // History: visible table and selected-record details side by side.
 control('History',5,0,2,'Reporting',['All records','Enabled','Opted out']);
 section('History',4,6,12,'Selected installation');
 text('History',5,6,12,"=IFNA(INDEX('Installation history'!A5:A20004,1),\"\")");
 fmt('History',5,6,6,12,{backgroundColorStyle:{rgbColor:C.light},textFormat:font(11,true,C.teal)});
 q.push({setDataValidation:{range:range('History',5,6,6,7),rule:{condition:{type:'ONE_OF_RANGE',values:[{userEnteredValue:"='Installation history'!A5:A20004"}]},strict:true,showCustomUi:true}}});
 header('History',6,0,['Installation','Version','Last active','Days / 30','Reporting']);
 cell('History',7,0,`=IFNA(FILTER({'Installation history'!A5:A20004,'Installation history'!C5:C20004,'Installation history'!E5:E20004,IF('Installation history'!E5:E20004="No activity",0,IF('Installation history'!D5:D20004="Unknown","Unknown",'Installation history'!G5:G20004)),'Installation history'!B5:B20004},'Installation history'!A5:A20004<>"",IF(A6="All records",'Installation history'!A5:A20004<>"",'Installation history'!B5:B20004=A6)),"No matching records")`);
 dim('History','COLUMNS',0,1,{pixelSize:180});dim('History','COLUMNS',1,2,{pixelSize:80});dim('History','COLUMNS',2,3,{pixelSize:120});dim('History','COLUMNS',3,4,{pixelSize:80});dim('History','COLUMNS',4,5,{pixelSize:100});dim('History','COLUMNS',5,6,{pixelSize:24});dim('History','COLUMNS',6,12,{pixelSize:60});
 const detail=(r,label,tab,col)=>{text('History',r,6,9,label);text('History',r,9,12,`=IFNA(VLOOKUP($G$6,'${tab}'!A5:K20004,${col},FALSE),"Unknown")`);fmt('History',r,r+1,6,9,{textFormat:font(10,false,C.muted)});};
 [[8,'Reporting','Installation history',2],[9,'Running version','Installation history',3],[10,'Installed version','Installation channels',4],[11,'Package','Installation channels',2],[12,'Update channel','Installation channels',3],[13,'First seen','Installation history',4],[14,'First desktop start','Installation history',9],[15,'Active days / 90','Installation history',8]].forEach(a=>detail(...a));
 section('History',17,6,12,'Version history');text('History',18,6,12,'=IFNA(SUBSTITUTE(VLOOKUP($G$6,\'Installation history\'!A5:K20004,11,FALSE)," → ",CHAR(10)),"No version history")');q.push({unmergeCells:{range:range('History',18,19,6,12)}},{mergeCells:{range:range('History',18,21,6,12),mergeType:'MERGE_ALL'}});
 section('History',22,6,12,'Channel history');text('History',23,6,12,'=IFNA(SUBSTITUTE(VLOOKUP($G$6,\'Installation channels\'!A5:F20004,6,FALSE)," → ",CHAR(10)),"No channel history")');q.push({unmergeCells:{range:range('History',23,24,6,12)}},{mergeCells:{range:range('History',23,26,6,12),mergeType:'MERGE_ALL'}});
 fmt('History',7,20020,3,4,{horizontalAlignment:'CENTER'});
 fmt('History',7,20020,2,3,{numberFormat:{type:'DATE',pattern:'d mmm yyyy'}});
 for(const r of [13,14])fmt('History',r,r+1,9,12,{numberFormat:{type:'DATE',pattern:'d mmm yyyy'}});
 for(const n of readers) {
  const offset=n==='Versions'?4:0;
  if(n==='History')q.push({addConditionalFormatRule:{index:0,rule:{ranges:[range(n,7,20020,0,5)],booleanRule:{condition:{type:'CUSTOM_FORMULA',values:[{userEnteredValue:'=AND($A8<>"",ISEVEN(ROW()))'}]},format:{backgroundColor:rgb('F5F8F9')}}}}});
  fmt(n,0,3,offset,offset+12,{wrapStrategy:'CLIP'});
 }
 for(const n of ['Installation channels','Channel versions','Channel activity','Version changes'])if(by[n])q.push({updateSheetProperties:{properties:{sheetId:id(n),hidden:true},fields:'hidden'}});
 // Sheets can discard series created before their formulas produce numbers.
 const charts=q.filter(r=>r.addChart);return {requests:[...q.filter(r=>!r.addChart),...charts]};
}
module.exports={build,OWNER};
if(require.main===module){const f=require('node:fs'),i=JSON.parse(f.readFileSync(0,'utf8'));process.stdout.write(JSON.stringify(build(i.metadata,i.inspected)));}
