'use strict';
// Builds a one-time, additive Sheets batch. No credentials, network or raw-data writes.
// Existing collector-owned ranges retain their names and values. Reruns refuse.
const OWNER = 'halcyon:presentation:v1';
const NEW_IDS = {Overview:804050001, Activity:804050002, History:804050003};
function build(metadata, inspected) {
  const sheets = metadata.sheets, byName=Object.fromEntries(sheets.map(s=>[s.properties.title,s.properties]));
  for(const name of ['Dashboard','Versions','Daily activity','Installation history','Countries','Summary','Reporting','Installations'])
    if(!byName[name]) throw Error('Missing collector sheet: '+name);
  for(const [name,id] of Object.entries(NEW_IDS)) {
    if(byName[name] || sheets.some(s=>s.properties.sheetId===id)) throw Error('Reader sheet already exists or ID is occupied: '+name);
  }
  const versions=inspected.sheets.find(s=>s.properties.title==='Versions');
  if(!versions) throw Error('Versions cells must be inspected before building');
  for(const d of versions.data||[]) for(const row of d.rowData||[]) for(const [j,c] of (row.values||[]).entries())
    if((d.startColumn||0)+j>=4 && (c.userEnteredValue || c.note || c.dataValidation)) throw Error('Occupied Versions presentation area');
  const ids={...Object.fromEntries(sheets.map(s=>[s.properties.title,s.properties.sheetId])),...NEW_IDS};
  const q=[], rgb=h=>({red:parseInt(h.slice(0,2),16)/255,green:parseInt(h.slice(2,4),16)/255,blue:parseInt(h.slice(4,6),16)/255});
  const C={ink:rgb('26343D'),muted:rgb('64727A'),header:rgb('EDF2F3'),accent:rgb('247B80'),second:rgb('7E9CAD'),line:rgb('DCE4E7'),white:rgb('FFFFFF')};
  const range=(s,r0,r1,c0,c1)=>({sheetId:ids[s],startRowIndex:r0,endRowIndex:r1,startColumnIndex:c0,endColumnIndex:c1});
  const cell=(s,r,c,value,note)=>q.push({updateCells:{range:range(s,r,r+1,c,c+1),rows:[{values:[{userEnteredValue:typeof value==='number'?{numberValue:value}:value.startsWith('=')?{formulaValue:value}:{stringValue:value},...(note?{note}:{})}]}],fields:'userEnteredValue'+(note?',note':'')}});
  const fmt=(s,r0,r1,c0,c1,f)=>q.push({repeatCell:{range:range(s,r0,r1,c0,c1),cell:{userEnteredFormat:f},fields:Object.keys(f).map(k=>'userEnteredFormat.'+k).join(',')}});
  const dim=(s,axis,start,end,p)=>q.push({updateDimensionProperties:{range:{sheetId:ids[s],dimension:axis,startIndex:start,endIndex:end},properties:p,fields:Object.keys(p).join(',')}});
  const merge=(s,r0,r1,c0,c1)=>q.push({mergeCells:{range:range(s,r0,r1,c0,c1),mergeType:'MERGE_ALL'}});
  const text=(s,r,c,end,value,note)=>{cell(s,r,c,value,note);if(end>c+1)merge(s,r,r+1,c,end);};
  const rule=(s,r,c,values,source)=>q.push({setDataValidation:{range:range(s,r,r+1,c,c+1),rule:{condition:source?{type:'ONE_OF_RANGE',values:[{userEnteredValue:source}]}:{type:'ONE_OF_LIST',values:values.map(userEnteredValue=>({userEnteredValue}))},strict:true,showCustomUi:true}}});
  const group=(s,start,end)=>{const r={sheetId:ids[s],dimension:'ROWS',startIndex:start,endIndex:end};q.push({addDimensionGroup:{range:r}},{updateDimensionGroup:{dimensionGroup:{range:r,depth:1,collapsed:true},fields:'collapsed'}});dim(s,'ROWS',start,end,{hiddenByUser:true});};
  const header=(s,r,c,labels)=>{labels.forEach((v,i)=>cell(s,r,c+i,v));fmt(s,r,r+1,c,c+labels.length,{backgroundColorStyle:{rgbColor:C.header},textFormat:{fontFamily:'Arial',fontSize:10,bold:true,foregroundColorStyle:{rgbColor:C.ink}},borders:{bottom:{style:'SOLID',colorStyle:{rgbColor:C.line}}}});dim(s,'ROWS',r,r+1,{pixelSize:42});};
  const metric=name=>'=IFNA(VLOOKUP("'+name+'",Dashboard!A5:B16,2,FALSE),"Unknown")';
  for(const [name,id] of Object.entries(NEW_IDS)) q.push({addSheet:{properties:{sheetId:id,title:name,hidden:true,gridProperties:{rowCount:name==='History'?20020:1000,columnCount:26,hideGridlines:true,frozenRowCount:2}}}});
  if(byName['Installation history'].gridProperties.rowCount<20004)q.push({updateSheetProperties:{properties:{sheetId:ids['Installation history'],gridProperties:{rowCount:20004}},fields:'gridProperties.rowCount'}});
  for(const name of ['Overview','Activity','Versions','History']) {
    const offset=name==='Versions'?4:0, cols=name==='History'?5:8, end=name==='History'?85:name==='Overview'?280:135;
    fmt(name,0,end,offset,offset+cols,{backgroundColorStyle:{rgbColor:C.white},textFormat:{fontFamily:'Arial',fontSize:11,foregroundColorStyle:{rgbColor:C.ink}},wrapStrategy:'WRAP',verticalAlignment:'MIDDLE'});
    dim(name,'ROWS',0,end,{pixelSize:25});dim(name,'ROWS',0,1,{pixelSize:42});dim(name,'ROWS',1,2,{pixelSize:35});
    dim(name,'COLUMNS',offset,offset+cols,{pixelSize:112});
    dim(name,'COLUMNS',offset+cols,26,{hiddenByUser:true});
    cell(name,0,25,OWNER);
    text(name,0,offset,offset+cols,name==='Overview'?'Halcyon · at a glance':name==='Activity'?'Activity over time':name==='History'?'Installation history':'Versions in use');
    fmt(name,0,1,offset,offset+cols,{textFormat:{fontFamily:'Arial',fontSize:21,bold:true,foregroundColorStyle:{rgbColor:C.ink}}});
    text(name,1,offset,offset+cols,'=IF(ISNUMBER(Reporting!B1),"As of "&TEXT(Reporting!B1,"d mmm yyyy")&" · UTC · installations, not people","Awaiting collector snapshot")');
    fmt(name,1,2,offset,offset+cols,{textFormat:{fontFamily:'Arial',fontSize:10,foregroundColorStyle:{rgbColor:C.muted}}});
  }
  // OVERVIEW: three metrics, two charts, expandable countries and coverage.
  for(const [c,end,label,m,hint] of [[0,3,'Active in 30 days ?', 'Active in 30 days','=Dashboard!B6&" also reported in the past 7 days"'],[3,6,'Reported on snapshot date ?', 'Active today','Today may still be in progress'],[6,8,'Used on 2+ days ?', 'Used on 2+ days this week','Within the past 7 days']]) {
    text('Overview',3,c,end,label,'Distinct enabled installation identities. Counts exclude opted-out records; this is not session duration or a people count.');
    cell('Overview',4,c,metric(m));merge('Overview',4,6,c,end);
    fmt('Overview',3,7,c,end,{backgroundColorStyle:{rgbColor:C.header}});
    fmt('Overview',4,6,c,end,{textFormat:{fontFamily:'Arial',fontSize:32,bold:true,foregroundColorStyle:{rgbColor:C.accent}}});
    text('Overview',6,c,end,hint);fmt('Overview',6,7,c,end,{textFormat:{fontFamily:'Arial',fontSize:10,foregroundColorStyle:{rgbColor:C.muted}}});
  }
  cell('Overview',3,3,'=IF(Reporting!B1=TODAY(),"Reported today ?","Reported "&TEXT(Reporting!B1,"d mmm")&" ?")');
  // A bounded, filtered helper removes untracked dates from the trend axis.
  cell('Overview',0,10,"=IFNA(FILTER('Daily activity'!A5:D94,ISNUMBER('Daily activity'!B5:B94)),\"\")");
  cell('Overview',0,15,"=IFNA(FILTER(Versions!A5:C25,Versions!A5:A25<>\"\"),\"\")");
  fmt('Overview',0,90,10,11,{numberFormat:{type:'DATE',pattern:'d mmm'}});
  [210,90,36].forEach((width,c)=>dim('Overview','COLUMNS',c,c+1,{pixelSize:width}));
  const receipt='Installations!O2:O20001';
  text('Overview',21,0,8,'=LET(t,'+receipt+',n,ARRAYFORMULA(IFERROR(IF(ISNUMBER(t),t,DATEVALUE(LEFT(t,10))+TIMEVALUE(MID(t,12,8))),0)),IF(MAX(n)=0,"No activity received yet","Last activity received: "&TEXT(MAX(n),"d mmm yyyy, hh:mm")&" UTC"))','Last successful activity receipt, not a guarantee of current collector uptime.');
  text('Overview',22,0,8,'=Dashboard!B14&" opted-out records excluded from activity · detailed history is not lifetime usage"');
  fmt('Overview',21,23,0,8,{textFormat:{fontFamily:'Arial',fontSize:10,foregroundColorStyle:{rgbColor:C.muted}}});
  text('Overview',23,0,8,'Countries & data coverage  ·  expand the row group for details');
  header('Overview',25,0,['Country','Active / 30 days']);
  const names={US:'United States',BR:'Brazil',DE:'Germany',RO:'Romania',CL:'Chile',DK:'Denmark',ES:'Spain',GB:'United Kingdom',HR:'Croatia',IN:'India',RS:'Serbia',SE:'Sweden',Unknown:'Not supplied'};
  const pairs=Object.entries(names).map(([k,v])=>'"'+k+'","'+v+'"').join(',');
  cell('Overview',26,0,'=IFNA(ARRAYFORMULA(FILTER({SWITCH(Countries!A2:A251,'+pairs+',Countries!A2:A251),Countries!D2:D251},Countries!A2:A251<>"")),"No country reports")');
  text('Overview',25,3,8,'What the numbers mean');
  const explanations=[
    ['Daily history', '=IFERROR("Tracked since "&TEXT(MIN(FILTER(\'Daily activity\'!A5:A94,ISNUMBER(\'Daily activity\'!B5:B94))),"d mmm yyyy")&". Earlier days are unknown.","No detailed history yet.")'],
    ['First seen ?', '=Dashboard!B15&" active records have an unknown first-seen date. First seen means first report since tracking began, not installation."'],
    ['First desktop start ?', '=Dashboard!B16&" active records supplied this date. Missing dates are never estimated."'],
    ['Retention', 'Up to 90 days. Opt-out and expiry can reduce earlier counts. Counts refer to retained installations.'],
    ['Collector tables', 'Source tables are hidden, not deleted. Use View → Hidden sheets to inspect them.']
  ];
  explanations.forEach(([label,value],i)=>{text('Overview',27+i*3,3,8,label);text('Overview',28+i*3,3,8,value);dim('Overview','ROWS',28+i*3,29+i*3,{pixelSize:44});});
  group('Overview',24,280);
  q.push({setBasicFilter:{filter:{range:range('Overview',25,280,0,8),criteria:{0:{condition:{type:'CUSTOM_FORMULA',values:[{userEnteredValue:'=OR($A27<>"",$D27<>"")'}]}}}}}});
  // ACTIVITY: filtered daily data, stable chart bounds, no zero-filling before history.
  text('Activity',3,0,1,'Window');text('Activity',3,1,3,'Past 7 days');rule('Activity',3,1,['Past 7 days','Past 30 days','Past 90 days']);
  text('Activity',3,3,8,'Detailed reporting only; earlier days stay unknown');
  header('Activity',20,0,['Date · UTC','Active','First reported day','Reported before']);
  cell('Activity',20,2,'First reported day ?','The first reporting day since detailed tracking began. This is not the date the app was downloaded or installed.');
  cell('Activity',20,3,'Reported before ?','A report on a later day than the installation\'s first observed reporting day.');
  cell('Activity',21,0,'=IFNA(FILTER(\'Daily activity\'!A5:D94,ISNUMBER(\'Daily activity\'!B5:B94),ARRAYFORMULA(IFERROR(IF(ISNUMBER(\'Daily activity\'!A5:A94),\'Daily activity\'!A5:A94,DATEVALUE(\'Daily activity\'!A5:A94)),0))>=Reporting!B1-VALUE(REGEXEXTRACT(B4,"[0-9]+"))+1),"No tracked reports in this window")');
  fmt('Activity',21,111,0,1,{numberFormat:{type:'DATE',pattern:'d mmm yyyy'}});
  fmt('Activity',21,111,1,4,{numberFormat:{type:'NUMBER',pattern:'#,##0'},horizontalAlignment:'RIGHT'});
  q.push({setBasicFilter:{filter:{range:range('Activity',20,111,0,4),criteria:{0:{condition:{type:'NOT_BLANK'}}}}}});
  text('Activity',115,0,8,'? Daily counts cannot be added to calculate unique installations. Today is incomplete; opt-outs and expiry can reduce earlier counts.');
  dim('Activity','ROWS',115,116,{pixelSize:44});
  text('Activity',118,0,8,'Return rates · expand for the available history');
  text('Activity',120,0,3,'Returned in week 2 ?');cell('Activity',120,3,metric('Returned in week 2'));
  text('Activity',122,0,3,'Returned in week 5 ?');cell('Activity',122,3,metric('Returned in week 5'));
  fmt('Activity',120,123,3,4,{numberFormat:{type:'PERCENT',pattern:'0.0%'}});
  text('Activity',124,0,8,'Full days 7–13 / 30–36 after first seen must elapse. Unknown and expired histories are excluded.');
  group('Activity',119,130);
  // VERSIONS: A:D stay collector-owned; the readable E:L region is independent.
  text('Versions',3,4,5,'Window');text('Versions',3,5,7,'Past 30 days');rule('Versions',3,5,['Past 7 days','Past 30 days']);
  header('Versions',6,4,['Version','Active installations','Share']);
  cell('Versions',7,4,'=IFNA(ARRAYFORMULA(FILTER({A5:A25,IF(F4="Past 7 days",B5:B25,C5:C25),IFERROR(IF(F4="Past 7 days",B5:B25,C5:C25)/SUM(IF(F4="Past 7 days",B5:B25,C5:C25)),0)},A5:A25<>"")),"No active installations")');
  fmt('Versions',7,28,5,6,{numberFormat:{type:'NUMBER',pattern:'#,##0'},horizontalAlignment:'RIGHT'});
  fmt('Versions',7,28,6,7,{numberFormat:{type:'PERCENT',pattern:'0.0%'},horizontalAlignment:'RIGHT'});
  text('Versions',30,4,12,'? Latest reported version per active installation. Older versions do not prove that an update failed.');
  text('Versions',32,4,12,'Observed changes for individual installations are available in History.');
  // HISTORY: exact-match selector; detail rows collapse independently of the table.
  [220,120,170,145,145].forEach((w,c)=>dim('History','COLUMNS',c,c+1,{pixelSize:w}));
  text('History',2,0,5,'Record details · choose below, then expand the + row group');
  fmt('History',2,3,0,5,{textFormat:{fontFamily:'Arial',fontSize:10,foregroundColorStyle:{rgbColor:C.muted}}});
  text('History',3,0,2,'=IF(COUNTIF(\'Installation history\'!A5:A20004,A16),A16,"")','Choose a record from the current filter, then expand the detail rows. If a manually selected record no longer matches the filter, choose another record.');
  rule('History',3,0,null,'=History!$A$16:$A$20015');
  text('History',3,2,3,'Reporting');text('History',3,3,5,'All records');rule('History',3,3,['All records','Enabled','Opted out']);
  const rawLookup=col=>'VLOOKUP($A$4,\'Installation history\'!$A$5:$K$20004,'+col+',FALSE)';
  const lookup=col=>'IF($A$4="","Choose an installation",IFNA(IF(OR($D$4="All records",'+rawLookup(2)+'=$D$4),'+rawLookup(col)+',"Choose a record in this filter"),"Record no longer retained"))';
  text('History',5,0,2,'First seen ?','First report since detailed tracking began; not the package installation date.');
  text('History',5,2,4,'First desktop start ?','Client-supplied first start. Unknown means it was not collected.');
  cell('History',5,4,'First observed version');
  text('History',6,0,2,'='+lookup(4));text('History',6,2,4,'='+lookup(9));cell('History',6,4,'='+lookup(10));
  fmt('History',6,7,0,4,{numberFormat:{type:'DATE',pattern:'d mmm yyyy'}});
  text('History',8,0,5,'=IF('+lookup(5)+'="No activity","No activity reported",IF(ISNUMBER('+lookup(4)+'),"Reported days · 7 days: "&'+lookup(6)+'&" · 30 days: "&'+lookup(7)+'&" · 90 days: "&'+lookup(8)+',IF('+lookup(4)+'="Unknown","Detailed day history is unknown",'+lookup(4)+')))');
  text('History',10,0,5,'Version observations ?','At most the last accepted version per UTC day since the version-history upgrade. This is not an exact package-installation timeline.');
  text('History',11,0,5,'='+lookup(11));dim('History','ROWS',11,12,{pixelSize:54});
  header('History',14,0,['Installation','Version','Last report · UTC','Days / 30 ?','Reporting']);
  cell('History',14,3,'Days / 30 ?','Distinct days actually received in the last 30 UTC days. Unknown means older day history was not collected. No activity records show zero.');
  const H="'Installation history'!";
  cell('History',15,0,'=IFNA(FILTER({'+H+'A5:A20004,'+H+'C5:C20004,'+H+'E5:E20004,ARRAYFORMULA(IF('+H+'E5:E20004="No activity",0,IF('+H+'D5:D20004="Unknown","Unknown",'+H+'G5:G20004))),'+H+'B5:B20004},'+H+'A5:A20004<>"",ARRAYFORMULA(IF($D$4="All records",'+H+'A5:A20004<>"",'+H+'B5:B20004=$D$4))),"No matching records")');
  fmt('History',15,20015,2,3,{numberFormat:{type:'DATE',pattern:'d mmm yyyy'}});
  fmt('History',15,20015,3,4,{numberFormat:{type:'NUMBER',pattern:'#,##0'},horizontalAlignment:'RIGHT'});
  group('History',4,13);
  for(const [v,bg,fg] of [['Enabled','EAF4EE','276044'],['Opted out','FFF4DF','78551D']])
    q.push({addConditionalFormatRule:{index:0,rule:{ranges:[range('History',15,20015,4,5)],booleanRule:{condition:{type:'TEXT_EQ',values:[{userEnteredValue:v}]},format:{backgroundColor:rgb(bg),textFormat:{foregroundColor:rgb(fg)}}}}}});
  // Native charts. Their sources are bounded and never depend on visible row order.
  function chart(id,target,row,col,width,height,title,subtitle,source,r0,r1,domain,series,type='COLUMN',stacked=false) {
    const src=c=>({sourceRange:{sources:[range(source,r0,r1,c,c+1)]}});
    q.push({addChart:{chart:{chartId:id,spec:{title,subtitle,fontName:'Arial',titleTextFormat:{fontSize:14,bold:true,foregroundColorStyle:{rgbColor:C.ink}},subtitleTextFormat:{fontSize:10,foregroundColorStyle:{rgbColor:C.muted}},backgroundColorStyle:{rgbColor:C.white},hiddenDimensionStrategy:'SHOW_ALL',basicChart:{chartType:type,legendPosition:series.length>1?'BOTTOM_LEGEND':'NO_LEGEND',headerCount:source==='Overview'?0:1,domains:[{domain:src(domain)}],series:series.map((c,i)=>({series:src(c),targetAxis:'LEFT_AXIS',colorStyle:{rgbColor:i?C.second:C.accent},...(series.length===1?{dataLabel:{type:'DATA'}}:{})})),axis:[{position:'BOTTOM_AXIS'},{position:'LEFT_AXIS',title:'Installations',viewWindowOptions:{viewWindowMin:0}}],...(stacked?{stackedType:'STACKED'}:{})}},position:{overlayPosition:{anchorCell:{sheetId:ids[target],rowIndex:row,columnIndex:col},widthPixels:width,heightPixels:height}}}}});
  }
  chart(804050101,'Overview',8,0,440,285,'Daily reports','UTC · today is incomplete','Overview',0,90,10,[11]);
  chart(804050102,'Overview',8,4,440,285,'Versions in use','Active in the past 30 days','Overview',0,21,15,[17]);
  chart(804050103,'Activity',5,0,890,345,'First reports & repeat reports','Unknown dates are omitted · today is incomplete','Activity',20,111,0,[2,3],'COLUMN',true);
  chart(804050104,'Versions',6,7,550,345,'Versions in use','Latest report within the selected window','Versions',6,28,4,[5]);
  // Preserve raw collector content: hide it only after the caller verifies formulas.
  const publish=[];
  for(const [i,name] of ['Overview','Activity','Versions','History'].entries())
    publish.push({updateSheetProperties:{properties:{sheetId:ids[name],index:i,hidden:false,gridProperties:{hideGridlines:true,frozenRowCount:name==='History'?15:2,frozenColumnCount:0}},fields:'index,hidden,gridProperties.hideGridlines,gridProperties.frozenRowCount,gridProperties.frozenColumnCount'}});
  for(const name of ['Dashboard','Daily activity','Installation history','Countries','Summary','Reporting','Installations'])
    publish.push({updateSheetProperties:{properties:{sheetId:ids[name],hidden:true},fields:'hidden'}});
  publish.push({updateDimensionProperties:{range:{sheetId:ids.Versions,dimension:'COLUMNS',startIndex:0,endIndex:4},properties:{hiddenByUser:true},fields:'hiddenByUser'}},
    {updateDimensionProperties:{range:{sheetId:ids.Versions,dimension:'ROWS',startIndex:0,endIndex:1},properties:{hiddenByUser:false},fields:'hiddenByUser'}},
    {clearBasicFilter:{sheetId:ids.Versions}});
  return {ids,stage:q,publish};
}
module.exports={build,OWNER,NEW_IDS};
if(require.main===module) {
  const fs=require('node:fs');
  const input=JSON.parse(fs.readFileSync(0,'utf8'));
  process.stdout.write(JSON.stringify(build(input.metadata,input.inspected)));
}

