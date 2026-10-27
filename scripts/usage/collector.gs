/* Halcyon collector. Deploy as owner, ANYONE_ANONYMOUS; Sheet stays private.
 * SPREADSHEET_ID remains in Script Properties. Never log bodies or identities.
 */
const MAX_INSTALLATIONS = 20000;
const RETENTION_DAYS = 90;
const WIDTH = 16;
const WRITE_BATCH_ROWS = 256;
const HEADERS = ['Installation hash','Last active (UTC)','Version','First reported country',
  'Reporting status','Opted out at (UTC)','Status received at (UTC)','Preference revision','Choice at (UTC)',
  'First activity received since tracking (UTC)','First desktop start (client UTC; if known)',
  'First version received since tracking','Reported active days (last 90 days)',
  'Activity days (UTC serials; internal)','Last activity received (UTC)',
  'Observed version changes (last 90 UTC days; internal)'];
function validCountry(value) { return typeof value === 'string' && /^[A-Z]{2}$/.test(value); }
function validVersion(v) { return typeof v === 'string' && v.length <= 48 && /^\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?$/.test(v); }
function validTime(t) { return Number.isSafeInteger(t) && t >= 0 && t <= 253402300799; }
function iso(value) {
  if (value === '' || value === undefined) return '';
  const date=value instanceof Date ? value : typeof value === 'string' ? new Date(value) : null;
  if (!date || !Number.isFinite(date.getTime()) || date.getTime()<0 || date.getTime()>253402300799999) throw Error('Invalid timestamp');
  const result=date.toISOString();
  if (!(value instanceof Date) && value!==result) throw Error('Invalid timestamp');
  return result;
}
function days_(value) {
  if (value === '' || value === undefined) return [];
  // Brackets keep a single day from being treated as a numeric Sheet cell.
  if (typeof value !== 'string' || value.length>800 || !/^\[\d{1,7}(,\d{1,7})*\]$/.test(value)) throw Error('Invalid activity days');
  const days=JSON.parse(value);
  if (days.length>RETENTION_DAYS || days.some((d,i)=>d<25569 || d>2958465 || i>0 && d<=days[i-1])) throw Error('Invalid activity days');
  return days;
}
function refreshDays_(row,today,extra=[]) {
  const days=[...new Set([...days_(row[13]),...extra])].filter(d=>d>=today-89 && d<=today).sort((a,b)=>a-b);
  row[13]=days.length ? JSON.stringify(days) : ''; row[12]=days.length || '';
}
function versions_(value) {
  if (value === '' || value === undefined) return [];
  if (typeof value !== 'string' || value.length>6000) throw Error('Invalid version history');
  const entries=JSON.parse(value);
  if (!Array.isArray(entries) || entries.length>90 || entries.some((e,i)=>
    !Array.isArray(e) || e.length!==2 || !Number.isSafeInteger(e[0]) || e[0]<25569 || e[0]>2958465
    || !validVersion(e[1]) || i>0 && e[0]<=entries[i-1][0])) throw Error('Invalid version history');
  return entries;
}
function refreshVersions_(row,today,extra=[]) {
  // At most the last observed version per UTC day. Duplicate rows merge in
  // authority order; neither a retry nor maintenance invents an observation.
  const entries=new Map([...versions_(row[15]),...extra]);
  const recent=[...entries].filter(([day])=>day>=today-89 && day<=today).sort((a,b)=>a[0]-b[0]);
  row[15]=recent.length ? JSON.stringify(recent) : '';
}
function validateReport(body) {
  if (typeof body !== 'string' || body.length > 1024) throw Error('Invalid report');
  const v = JSON.parse(body);
  if (!v || Array.isArray(v) || typeof v.installation !== 'string' || !/^[a-f0-9]{64}$/.test(v.installation)) throw Error('Invalid report');
  const keys = Object.keys(v).sort().join(',');
  const legacy = v.schema === 1 && keys === 'installation,schema,version' && validVersion(v.version)
    || v.schema === 2 && keys === 'country,installation,schema,version' && validVersion(v.version) && (v.country === null || validCountry(v.country));
  const revision = Number.isSafeInteger(v.revision) && v.revision >= 0;
  const activity = v.schema === 3 && v.kind === 'activity' && keys === 'country,installation,kind,revision,schema,version'
    && revision && validVersion(v.version) && (v.country === null || validCountry(v.country));
  const detailed = v.schema === 4 && v.kind === 'activity' && keys === 'country,first_started_at,installation,kind,revision,schema,version'
    && revision && validVersion(v.version) && (v.country === null || validCountry(v.country))
    && (v.first_started_at === null || validTime(v.first_started_at));
  // Client action time is informational, never a receipt/retention clock. Accept
  // skew within the representable UTC range; revision alone orders choices.
  const status = v.schema === 3 && v.kind === 'status' && keys === 'changed_at,enabled,installation,kind,revision,schema'
    && revision && v.revision > 0 && typeof v.enabled === 'boolean'
    && Number.isSafeInteger(v.changed_at) && v.changed_at >= 0 && v.changed_at <= 253402300799;
  if (!(legacy || activity || detailed || status)) throw Error('Invalid report');
  return v;
}
function dayNumber(date) { return Math.floor(date.getTime() / 86400000) + 25569; }
function time(value) { return value ? new Date(value).getTime() : NaN; }
function normalize(row) {
  const active = row[1] instanceof Date ? dayNumber(row[1]) : (row[1] === '' ? '' : Number(row[1]));
  return [row[0], active, row[2] || '', validCountry(row[3]) ? row[3] : '',
    row[4] || 'enabled', row[5] instanceof Date ? row[5].toISOString() : row[5] || '',
    row[6] instanceof Date ? row[6].toISOString() : row[6] || '', Number(row[7] || 0),
    row[8] instanceof Date ? row[8].toISOString() : row[8] || '',
    iso(row[9]),iso(row[10]),row[11] || '',row[12] || '',row[13] || '',iso(row[14]),row[15] || ''];
}
function currentRows(rows, today) {
  const unique = new Map();
  for (const raw of rows) {
    if (!raw[0]) continue;
    const row = normalize(raw), previous = unique.get(row[0]);
    // A prefix write can persist the first receipt before its paired version.
    // Repair from that row's saved version before deduplication or a newer
    // report replaces it; absent legacy observations remain unknown.
    if (row[9] && !row[11] && validVersion(row[2])) row[11]=row[2];
    if (!previous) { unique.set(row[0], row); continue; }
    // A repeated/partially compacted row must not inflate active-day counts or
    // replace the earliest observation with the date of its retry.
    const days=[...days_(row[13]),...days_(previous[13])];
    const first = !previous[9] || row[9] && row[9]<previous[9] ? row : previous;
    const started=[row[10],previous[10]].filter(Boolean).sort()[0] || '';
    const latest=[row[14],previous[14]].filter(Boolean).sort().pop() || '';
    const metadata=[first[9],started,first[11],'','',latest];
    // A committed higher revision is authoritative even if a failed compaction
    // left an older row later in the sheet. Equal-revision opt-out wins closed.
    const stronger = row[7] > previous[7] || row[7] === previous[7] && row[4] === 'opted_out' && previous[4] !== 'opted_out';
    if (stronger || row[7] === previous[7] && row[4] === previous[4]
      && (Number(row[1]) > Number(previous[1]) || row[1]===previous[1] && time(row[14])>time(previous[14]))) {
      row[3] = previous[3] || row[3]; unique.set(row[0], row);
    } else if (!previous[3]) previous[3] = row[3];
    const selected=unique.get(row[0]); selected.splice(9,6,...metadata); refreshDays_(selected,today,days);
    const other=selected===row ? previous : row;
    const history=versions_(selected[15]); selected[15]=other[15]; refreshVersions_(selected,today,history);
  }
  return Array.from(unique.values()).filter(row => {
    refreshDays_(row,today);
    refreshVersions_(row,today);
    const receivedDay = Number.isFinite(time(row[6])) ? dayNumber(new Date(row[6])) : 0;
    // Opted-out rows expire from status receipt, never from action time or an
    // old client's replay. Enabled records also retain their recent activity.
    const retained = row[4] === 'opted_out' ? receivedDay : Math.max(Number(row[1]) || 0, receivedDay);
    if (retained < today - RETENTION_DAYS + 1) return false;
    if (row[4] !== 'opted_out' && receivedDay < today - RETENTION_DAYS + 1) {
      row[5] = ''; row[6] = ''; row[8] = ''; // age out identifiable choice timestamps
    }
    return true;
  });
}
function applyReport(rows, id, report, now) {
  const today = dayNumber(now), next = currentRows(rows, today);
  let row = next.find(r => r[0] === id);
  if (report.kind === 'status') {
    if (row && report.revision < row[7]) return next;
    if (row && report.revision === row[7]) {
      // Exact retries are no-ops: neither receipt nor retention is refreshed.
      if (row[4] !== (report.enabled ? 'enabled' : 'opted_out') || row[8] && row[8] !== new Date(report.changed_at*1000).toISOString()) throw Error('Revision conflict');
      return next;
    }
    if (!row) {
      if (next.length >= MAX_INSTALLATIONS) throw Error('Capacity reached');
      row = normalize([id,'','','','enabled','','',0,'']); next.push(row);
    }
    row[4] = report.enabled ? 'enabled' : 'opted_out';
    row[8] = new Date(report.changed_at * 1000).toISOString();
    if (!report.enabled) row[5] = row[8];
    row[6] = now.toISOString(); row[7] = report.revision;
    return next; // status never touches active date, country or app version
  }
  if (row && (row[4] !== 'enabled' || report.schema >= 3 && report.revision !== row[7])) return next;
  // Old clients may continue enabled activity but never change preference state.
  if (!row) {
    if (next.length >= MAX_INSTALLATIONS) throw Error('Capacity reached');
    row = normalize([id,'','','','enabled','','',report.revision || 0,'']); next.push(row);
  }
  row[1] = today; row[2] = report.version;
  if (!row[3]) row[3] = validCountry(report.country) ? report.country : '';
  if (!row[9]) { row[9]=now.toISOString(); row[11]=report.version; }
  if (!row[10] && report.schema === 4 && report.first_started_at !== null)
    row[10]=new Date(report.first_started_at*1000).toISOString();
  row[14]=now.toISOString(); refreshDays_(row,today,[today]);
  const history=versions_(row[15]);
  if (!history.length || history[history.length-1][1]!==report.version)
    refreshVersions_(row,today,[[today,report.version]]);
  return next;
}
function updateRows(rows, id, version, today, country = null) {
  return applyReport(rows, id, {schema:2,version,country}, new Date((today-25569)*86400000));
}
function countryCounts(rows, today) {
  const countries = new Map();
  for (const row of currentRows(rows, today)) {
    if (row[4] !== 'enabled' || row[1] === '' || row[1] > today || row[1] < today-29) continue;
    const country = row[3] || 'Unknown';
    if (!countries.has(country)) countries.set(country, [country,0,0,0]);
    const counts = countries.get(country);
    if (row[1] === today) counts[1]++;
    if (row[1] >= today-6) counts[2]++;
    counts[3]++;
  }
  return Array.from(countries.values()).sort((a,b)=>b[3]-a[3] || a[0].localeCompare(b[0]));
}
function countries_(book, rows, today) {
  const sheet = book.getSheetByName('Countries') || book.insertSheet('Countries');
  // Label the snapshot date so a quiet period or delayed trigger cannot look current.
  const date = new Date((today-25569)*86400000).toISOString().slice(0,10);
  const values = [['First reported country', 'Active '+date+' (UTC)', 'Last 7 days', 'Last 30 days'],
    ...countryCounts(rows, today)];
  const count = Math.max(values.length, sheet.getLastRow());
  if (sheet.getMaxRows() < count) sheet.insertRowsAfter(sheet.getMaxRows(), count-sheet.getMaxRows());
  while (values.length < count) values.push(['','','','']);
  sheet.getRange(1,1,count,4).setValues(values);
  sheet.setFrozenRows(1);
}
function book_() {
  const id = PropertiesService.getScriptProperties().getProperty('SPREADSHEET_ID');
  if (!id) throw new Error('Collector not configured');
  return SpreadsheetApp.openById(id);
}
// A header alone is not permission to consume owner data. Read both values
// and formulas before any migration/ingestion/maintenance write. Missing headers
// may only cover empty columns; recognized headings require coherent state.
function statusPlan_(sheet, requireHeaders = false) {
  const count = Math.max(1,sheet.getLastRow()), width = Math.min(WIDTH,sheet.getMaxColumns());
  if (count > MAX_INSTALLATIONS+1) throw Error('Capacity reached');
  const range=sheet.getRange(1,1,count,width);
  const pad=row=>Array.from({length:WIDTH},(_,i)=>row[i] ?? '');
  const values=range.getValues().map(pad),formulas=range.getFormulas().map(pad);
  for (let col=4;col<WIDTH;col++) {
    if (formulas.some(row=>row[col] !== '')) throw Error('Formula in reporting status column');
    const header=values[0][col];
    if (header !== HEADERS[col] && header !== '') throw Error('Unrecognized existing status column');
    if (header === '' && (requireHeaders || values.slice(1).some(row=>row[col] !== '')))
      throw Error('Occupied or unmigrated reporting status column');
  }
  const initialize=[];
  for (let i=1;i<values.length;i++) {
    const row=values[i],v=row.slice(4,9),empty=row.slice(4).every(value=>value === '');
    // Added metadata columns are optional for migrated rows; never backdate
    // observation or infer a first start/version from the old last-active cell.
    [9,10,14].forEach(col=>iso(row[col]));
    if (row[11]!=='' && !validVersion(row[11])) throw Error('Invalid first version');
    days_(row[13]);
    versions_(row[15]);
    if (row[12]!=='' && (!Number.isSafeInteger(row[12]) || row[12]<0 || row[12]>RETENTION_DAYS)) throw Error('Invalid active-day count');
    if (!row[0]) {
      if (!empty) throw Error('Reporting status without an installation');
      continue;
    }
    // A prefix of the default-state write is safe to finish. Never infer an
    // off status, timestamp or positive revision from incomplete cells.
    if ((v[0] === '' || v[0] === 'enabled') && (v[3] === '' || v[3] === 0)
        && v[1] === '' && v[2] === '' && v[4] === '') {
      if (v[0] !== 'enabled' || v[3] !== 0) initialize.push([i+1,['enabled','','',0,'']]);
      continue;
    }
    const timestamp=value=>{
      if (value === '') return '';
      const date=value instanceof Date ? value : typeof value === 'string' ? new Date(value) : null;
      if (!date || !Number.isFinite(date.getTime()) || date.getTime()<0 || date.getTime()>253402300799999) return null;
      const iso=date.toISOString();
      return value instanceof Date || value === iso ? iso : null;
    };
    const opted=timestamp(v[1]),received=timestamp(v[2]),choice=timestamp(v[4]),revision=v[3];
    if (!['enabled','opted_out'].includes(v[0]) || !Number.isSafeInteger(revision) || revision<=0
        || [opted,received,choice].includes(null) || Boolean(received)!==Boolean(choice)
        || (!received && opted) || (v[0] === 'opted_out' && (!received || !opted || opted!==choice)))
      throw Error('Incoherent reporting status');
  }
  return {values,formulas,initialize,rows:values.slice(1).map(normalize)};
}
function rows_(sheet) { return statusPlan_(sheet,true).rows; }
const REPORTING_OWNER = 'halcyon:reporting:schema3:v1';
const REPORTING_LABELS = ['Reporting snapshot (UTC)','Active today (reporting enabled)',
  'Active last 7 days (reporting enabled)','Active last 30 days (reporting enabled)',
  'Currently opted out (status received in last 90 days)','Counts are retained installations, not lifetime users'];
// A8 is outside the A1:B6 snapshot. An unmarked nonempty tab is never adopted,
// even if its labels happen to resemble our snapshot or formulas display blank.
function reporting_(book) {
  const sheet=book.getSheetByName('Reporting');
  if (!sheet) return null;
  if (sheet.getMaxRows()>=8) {
    const marker=sheet.getRange(8,1,1,1);
    if (marker.getValues()[0][0] === REPORTING_OWNER && marker.getFormulas()[0][0] === '') {
      if (sheet.getMaxColumns()<2) throw Error('Invalid Reporting snapshot');
      const snapshot=sheet.getRange(1,1,6,2);
      if (snapshot.getFormulas().some(row=>row.some(Boolean))) throw Error('Formula in Reporting snapshot');
      snapshot.getValues().forEach(([label,value],i)=>{
        // Accept empty prefixes left by interruption, but not owner edits inside
        // the reserved snapshot. A copied marker alone cannot authorize them.
        // Sheets may coerce the ISO snapshot day written by setValues into a
        // native Date cell. Accept that owned date on the next request too.
        const validValue=value === '' || (i === 0 ? (typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value)
          || value instanceof Date && Number.isFinite(value.getTime()))
          : i<5 && Number.isSafeInteger(value) && value>=0);
        if ((label !== REPORTING_LABELS[i] && (label !== '' || value !== '')) || !validValue)
          throw Error('Unrecognized Reporting snapshot contents');
      });
      return sheet;
    }
  }
  if (sheet.getLastRow() !== 0) throw Error('Reporting destination is not collector-owned or empty');
  return sheet;
}
function claimReporting_(book) {
  const sheet=reporting_(book) || book.insertSheet('Reporting');
  if (sheet.getMaxRows()<8) sheet.insertRowsAfter(sheet.getMaxRows(),8-sheet.getMaxRows());
  if (sheet.getMaxColumns()<2) sheet.insertColumnsAfter(sheet.getMaxColumns(),2-sheet.getMaxColumns());
  const marker=sheet.getRange(8,1,1,1);
  if (marker.getValues()[0][0] !== REPORTING_OWNER) marker.setValue(REPORTING_OWNER);
  return sheet;
}
function write_(sheet, before, after) {
  const write = (first, rows) => {
    for (let offset=0;offset<rows.length;offset+=WRITE_BATCH_ROWS) {
      const batch=rows.slice(offset,offset+WRITE_BATCH_ROWS);
      sheet.getRange(first+offset,1,batch.length,WIDTH).setValues(batch);
    }
  };
  if (after.length === before.length && after.every((row,i)=>row[0] === before[i][0])) {
    // Expiry can change every retained row. Coalesce adjacent changes, retaining
    // the small single-row write for an ordinary report with no maintenance.
    let first=0;
    while (first<after.length) {
      if (JSON.stringify(after[first]) === JSON.stringify(before[first])) { first++; continue; }
      let end=first+1;
      while (end<after.length && JSON.stringify(after[end]) !== JSON.stringify(before[end])) end++;
      write(first+2,after.slice(first,end)); first=end;
    }
  } else {
    if (sheet.getMaxRows() < after.length+1) sheet.insertRowsAfter(sheet.getMaxRows(),after.length+1-sheet.getMaxRows());
    const values = after.concat(Array.from({length:Math.max(0,before.length-after.length)},()=>Array(WIDTH).fill('')));
    write(2,values);
  }
  SpreadsheetApp.flush();
}
function summary_(book, rows, today) {
  // Dedicated current snapshot. Do not overwrite existing historical aggregates
  // or any other Summary cells/formulas/layout owned by the private workbook.
  const sheet = claimReporting_(book);
  const current = currentRows(rows,today), enabled = current.filter(r=>r[4] === 'enabled' && r[1] !== '' && r[1] <= today);
  const values=[new Date((today-25569)*86400000).toISOString().slice(0,10),
    enabled.filter(r=>r[1]===today).length,enabled.filter(r=>r[1]>=today-6).length,
    enabled.filter(r=>r[1]>=today-29).length,current.filter(r=>r[4]==='opted_out').length,''];
  sheet.getRange(1,1,6,2).setValues(REPORTING_LABELS.map((label,i)=>[label,values[i]]));
}
const ANALYTICS_OWNER='halcyon:analytics:v1';
const ANALYTICS_TABS={
  'Dashboard':['Metric','Installations / value','Meaning'],
  'Versions':['Version','Active last 7 days','Active last 30 days','Share of active last 30 days'],
  'Daily activity':['UTC date','Reported active','First seen','Returning'],
  'Installation history':['Installation','Reporting','Last version','First seen (UTC)','Last active (UTC)',
    'Active days / 7','Active days / 30','Active days / 90','First desktop start (client UTC)',
    'First observed version','Observed version changes'],
};
function utcDay_(serial) { return new Date((serial-25569)*86400000).toISOString().slice(0,10); }
function analytics_(rows,today) {
  const current=currentRows(rows,today),enabled=current.filter(r=>r[4]==='enabled' && r[1]!=='' && r[1]<=today);
  const active=days=>enabled.filter(r=>r[1]>=today-days+1);
  const first=r=>r[9] ? dayNumber(new Date(r[9])) : null;
  const days=(r,n)=>days_(r[13]).filter(d=>d>=today-n+1).length;
  const last30=active(30),tracked30=last30.filter(r=>first(r)!==null);
  const new7=enabled.filter(r=>first(r)!==null && first(r)>=today-6 && first(r)<=today).length;
  const returning7=active(7).filter(r=>first(r)!==null && first(r)<today-6).length;
  // Only fully elapsed return windows whose history still fits retention.
  // This is observed return among currently enabled, retained installations,
  // not lifetime user retention or an estimate of activity while offline.
  const retention=(start,end)=>{
    const eligible=enabled.filter(r=>first(r)!==null && first(r)>=today-89 && first(r)+end<today);
    const returned=eligible.filter(r=>days_(r[13]).some(d=>d>=first(r)+start && d<=first(r)+end)).length;
    return [eligible.length ? returned/eligible.length : 'Not enough history',
      eligible.length ? `${returned} of ${eligible.length} reported again on days ${start}–${end} after first seen; currently enabled only.`
        : `Waiting for complete days ${start}–${end} after first seen.`];
  };
  const week=retention(7,13),month=retention(30,36);
  const dashboard=[
    ['Active today',active(1).length,'Reported today; today is still in progress.'],
    ['Active in 7 days',active(7).length,'Distinct installations reporting in the last 7 UTC days.'],
    ['Active in 30 days',last30.length,'Distinct installations reporting in the last 30 UTC days.'],
    ['First seen in 7 days',new7,'First report received since detailed tracking began; not downloads or installs.'],
    ['Returning in 7 days',returning7,'Active this week and first seen before this 7-day window.'],
    ['Used on 2+ days this week',active(7).filter(r=>days(r,7)>=2).length,'At least two distinct reported UTC days.'],
    ['Average reported days / 30',tracked30.length ? tracked30.reduce((n,r)=>n+days(r,30),0)/tracked30.length : 'No reports',
      'Among active installations with day history; missing and offline days are not reconstructed.'],
    ['Returned in week 2',...week],['Returned in week 5',...month],
    ['Currently opted out',current.filter(r=>r[4]==='opted_out').length,'Preference received in the retained 90-day window.'],
    ['Active with unknown first seen',last30.filter(r=>first(r)===null).length,'Older records; excluded from new/returning and retention metrics.'],
    ['Known first desktop start',last30.filter(r=>r[10]).length,'Client-supplied timestamp, not package installation time.'],
  ];
  const versions=new Map();
  for (const row of last30) {
    const name=validVersion(row[2]) ? row[2] : 'Unknown';
    if (!versions.has(name)) versions.set(name,[name,0,0,0]);
    const v=versions.get(name);if(row[1]>=today-6) v[1]++;v[2]++;
  }
  const versionRows=[...versions.values()].sort((a,b)=>b[2]-a[2] || a[0].localeCompare(b[0]));
  if(versionRows.length>20) {
    const rest=versionRows.splice(20);
    versionRows.push(['Other versions',rest.reduce((n,r)=>n+r[1],0),rest.reduce((n,r)=>n+r[2],0),0]);
  }
  versionRows.forEach(r=>{r[3]=r[2]/last30.length;});
  const daily=Array.from({length:90},(_,i)=>[utcDay_(today-89+i),0,0,0]);
  let earliest=today+1;
  for (const row of enabled) {
    const observed=first(row); if(observed===null) continue;
    earliest=Math.min(earliest,observed);
    for (const day of days_(row[13])) {
      const item=daily[day-(today-89)];item[1]++;
      if(day===observed) item[2]++;else item[3]++;
    }
  }
  daily.forEach((r,i)=>{if(today-89+i<earliest) r.splice(1,3,'','','');});
  const prefixes=new Map();current.forEach(r=>prefixes.set(String(r[0]).slice(0,12),(prefixes.get(String(r[0]).slice(0,12))||0)+1));
  // Keep source order so an ordinary report changes one installation row,
  // rather than moving the entire table on every last-active update.
  const history=current.map(r=>{
    const prefix=String(r[0]).slice(0,12),observed=first(r);
    return ['ID '+(prefixes.get(prefix)>1 ? r[0] : prefix),r[4]==='enabled'?'Enabled':'Opted out',validVersion(r[2])?r[2]:'Unknown',
      observed===null?'Unknown':utcDay_(observed),r[1]===''?'No activity':utcDay_(r[1]),
      days(r,7),days(r,30),days_(r[13]).length,r[10] ? r[10].slice(0,10) : 'Unknown',r[11] || 'Unknown',
      versions_(r[15]).map(([day,v])=>`${utcDay_(day)}: ${v}`).join(' → ') || 'No version history yet'];
  });
  return {'Dashboard':dashboard,'Versions':versionRows,'Daily activity':daily,'Installation history':history};
}
// New generated tabs are separate from the owner's Summary. Only an empty tab
// can be claimed. Reserve A:width, rows 1 onward; other columns remain untouched.
// A1 is an explicit ownership marker, row 2 a dated explanation, row 4 headers.
function analyticsPreflight_(book) {
  const plan={};
  for(const [name,headers] of Object.entries(ANALYTICS_TABS)) {
    const sheet=book.getSheetByName(name);plan[name]=[];
    if(!sheet || sheet.getLastRow()===0) continue;
    if(sheet.getLastRow()>MAX_INSTALLATIONS+4 || sheet.getMaxColumns()<headers.length || sheet.getMaxRows()<4)
      throw Error('Invalid analytics destination');
    const marker=sheet.getRange(1,1);
    if(marker.getValues()[0][0]!==ANALYTICS_OWNER || marker.getFormulas()[0][0]!=='')
      throw Error('Analytics destination is not collector-owned');
    const range=sheet.getRange(1,1,Math.max(4,sheet.getLastRow()),headers.length);
    if(range.getFormulas().some(r=>r.some(Boolean))) throw Error('Formula in generated analytics');
    const values=range.getValues(),labels=values[3];plan[name]=values;
    if(labels.some((value,i)=>value!=='' && value!==headers[i])) throw Error('Edited analytics headers');
  }
  return plan;
}
function analyticsWrite_(book,rows,today,plan) {
  const reports=analytics_(rows,today);
  // Sheets sometimes coerces ISO days into native Date cells. Compare their
  // UTC day, otherwise unchanged tables would be rewritten on every report.
  const same=(a,b)=>JSON.stringify(a && a.map(v=>v instanceof Date ? v.toISOString().slice(0,10) : v))
    ===JSON.stringify(b && b.map(v=>v instanceof Date ? v.toISOString().slice(0,10) : v));
  for(const [name,body] of Object.entries(reports)) {
    const headers=ANALYTICS_TABS[name],width=headers.length;
    const sheet=book.getSheetByName(name) || book.insertSheet(name);
    if(sheet.getMaxColumns()<width) sheet.insertColumnsAfter(sheet.getMaxColumns(),width-sheet.getMaxColumns());
    const note=name==='Installation history'
      ? 'Retained installation records. Version changes start with this collector upgrade; at most the last version per UTC day.'
      : 'Currently enabled, retained installations. Missing reports are not backfilled. Historical counts can fall after opt-out or expiry.';
    const values=[[ANALYTICS_OWNER],[`Updated ${utcDay_(today)} (UTC). ${note}`],[],headers,...body]
      .map(r=>Array.from({length:width},(_,i)=>r[i]===undefined?'':r[i]));
    const count=Math.max(values.length,sheet.getLastRow());
    if(sheet.getMaxRows()<count) sheet.insertRowsAfter(sheet.getMaxRows(),count-sheet.getMaxRows());
    // Claim before body writes so an interrupted first refresh is retryable.
    const before=plan[name];
    if(!before.length) sheet.getRange(1,1).setValue(ANALYTICS_OWNER);
    while(values.length<count) values.push(Array(width).fill(''));
    for(let offset=0;offset<values.length;) {
      if(same(values[offset],before[offset])) {offset++;continue;}
      let end=offset+1;
      while(end<values.length && end-offset<WRITE_BATCH_ROWS && !same(values[end],before[end])) end++;
      sheet.getRange(offset+1,1,end-offset,width).setValues(values.slice(offset,end));offset=end;
    }
    if(!before.length) sheet.setFrozenRows(4);
  }
}
function json_(value) { return ContentService.createTextOutput(JSON.stringify(value)).setMimeType(ContentService.MimeType.JSON); }
function doGet() { return json_({service:'halcyon-activity',schema:4}); }
function doPost(event) {
  let lock;
  try {
    const report = validateReport(event && event.postData && event.postData.contents);
    const id = Utilities.computeDigest(Utilities.DigestAlgorithm.SHA_256,report.installation)
      .map(b=>('0'+((b+256)%256).toString(16)).slice(-2)).join('');
    lock = LockService.getScriptLock();
    if (!lock.tryLock(3000)) return json_({ok:false});
    const now=new Date(),today=dayNumber(now),book=book_(),sheet=book.getSheetByName('Installations');
    const before=rows_(sheet); reporting_(book); const analytics=analyticsPreflight_(book); // read-only preflight before any write
    const after=applyReport(before,id,report,now);
    write_(sheet,before,after);
    countries_(book,after,today); summary_(book,after,today);
    analyticsWrite_(book,after,today,analytics);
    const row=after.find(r=>r[0]===id);
    return json_(report.kind === 'status' ? {ok:true,revision:row[7],enabled:row[4]==='enabled'}
      : {ok:true,day:Utilities.formatDate(now,'UTC','yyyy-MM-dd')});
  } catch (_) { return json_({ok:false}); }
  finally { if (lock && lock.hasLock()) lock.releaseLock(); }
}
function maintenance() {
  const lock=LockService.getScriptLock(); if (!lock.tryLock(10000)) return;
  try {
    const book=book_(),sheet=book.getSheetByName('Installations'),before=rows_(sheet),today=dayNumber(new Date());
    reporting_(book); const analytics=analyticsPreflight_(book); // collisions must leave even Installations untouched
    const after=currentRows(before,today); write_(sheet,before,after);
    countries_(book,after,today); summary_(book,after,today);
    analyticsWrite_(book,after,today,analytics);
  } finally { lock.releaseLock(); }
}
// Upgrade only the three documented current-count formulas wherever the owner
// placed them. Leave custom formulas, formatting and historical values intact.
function migrateSummary_(sheet) {
  const formulas = sheet.getDataRange().getFormulas();
  const originals = [0,6,29].map(days => '=COUNTIFS(Installations!B2:B20001,">="&TODAY()'+(days ? '-'+days : '')+',Installations!B2:B20001,"<="&TODAY())');
  formulas.forEach((row,i)=>row.forEach((formula,j)=>{
    if (originals.includes(formula)) sheet.getRange(i+1,j+1).setFormula(formula.slice(0,-1)+',Installations!E2:E20001,"enabled")');
  }));
}
// Schema-only migration: append columns, preserve every original row and all
// workbook properties. No deletion, timezone change, sorting or pruning here.
function setup() {
  const lock=LockService.getScriptLock(); if (!lock.tryLock(10000)) throw Error('Busy');
  try {
    const book=book_(),raw=book.getSheetByName('Installations');
    const plan=statusPlan_(raw);
    reporting_(book); const analytics=analyticsPreflight_(book); // finish all preflights before changing any workbook cell
    claimReporting_(book); // recoverable even if the next write is interrupted
    if (raw.getMaxColumns() < WIDTH) raw.insertColumnsAfter(raw.getMaxColumns(),WIDTH-raw.getMaxColumns());
    if (raw.getMaxRows() < MAX_INSTALLATIONS+1) raw.insertRowsAfter(raw.getMaxRows(),MAX_INSTALLATIONS+1-raw.getMaxRows());
    raw.getRange(1,5,1,WIDTH-4).setValues([HEADERS.slice(4)]);
    if (plan.values[0][3] === '' && plan.formulas[0][3] === '') raw.getRange(1,4).setValue(HEADERS[3]);
    // Initialize only empty/default prefixes. Valid status values (including
    // Date cells) and all original A:D cells remain exactly as they were.
    // Batch adjacent legacy rows instead of issuing one Sheets call per row.
    for (let i=0;i<plan.initialize.length;) {
      const row=plan.initialize[i][0],values=[];
      do { values.push(plan.initialize[i++][1]); }
      while (i<plan.initialize.length && plan.initialize[i][0] === row+values.length);
      raw.getRange(row,5,values.length,5).setValues(values);
    }
    const rows=plan.rows;
    SpreadsheetApp.flush();
    if (!ScriptApp.getProjectTriggers().some(t=>t.getHandlerFunction()==='maintenance'))
      ScriptApp.newTrigger('maintenance').timeBased().everyDays(1).atHour(3).create();
    countries_(book,rows,dayNumber(new Date())); summary_(book,rows,dayNumber(new Date()));
    analyticsWrite_(book,rows,dayNumber(new Date()),analytics);
    migrateSummary_(book.getSheetByName('Summary'));
    book.getSheetByName('Summary').getRange('B9').setValue('Configured');
    return {configured:true};
  } finally { lock.releaseLock(); }
}
if (typeof module !== 'undefined') module.exports = {validateReport,updateRows,applyReport,currentRows};
