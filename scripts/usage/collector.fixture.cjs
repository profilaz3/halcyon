// Local Google Sheets boundary: stored formulas and their evaluated values are
// distinct. Range writes erase formulas only in the cells actually written.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const vm = require('node:vm');
class Sheet {
  constructor(name, state, maxRows = 1000, maxColumns = 26) {
    Object.assign(this, {name,state,maxRows,maxColumns,frozenRows:0,cells:new Map()});
  }
  cell(r,c) { return this.cells.get(`${r},${c}`) || {value:'',formula:''}; }
  put(r,c,value,formula='') { this.cells.set(`${r},${c}`,{value,formula}); }
  getMaxRows() { return this.maxRows; }
  getMaxColumns() { return this.maxColumns; }
  insertRowsAfter(_,n) { this.maxRows += n; this.state.writes++; }
  insertColumnsAfter(_,n) { this.maxColumns += n; this.state.writes++; }
  setFrozenRows(n) { this.frozenRows=n; this.state.writes++; }
  getLastRow() { return Math.max(0,...[...this.cells].filter(([,v])=>v.value!=='' || v.formula!=='').map(([k])=>+k.split(',')[0])); }
  getLastColumn() { return Math.max(0,...[...this.cells].filter(([,v])=>v.value!=='' || v.formula!=='').map(([k])=>+k.split(',')[1])); }
  getDataRange() { return this.getRange(1,1,Math.max(1,this.getLastRow()),Math.max(1,this.getLastColumn())); }
  getRange(r,c,h=1,w=1) {
    if (typeof r==='string') { assert.equal(r,'B9');r=9;c=2; }
    assert.ok(r+h-1<=this.maxRows && c+w-1<=this.maxColumns,'Range outside allocated sheet');
    const write = (values, formulas) => {
      assert.equal(values.length,h);
      this.state.writes++;
      values.forEach((row,i)=>{
        assert.equal(row.length,w);
        row.forEach((value,j)=>{
          this.put(r+i,c+j,value,formulas ? formulas[i][j] : '');
          if (this.state.cellsUntilFailure !== null && --this.state.cellsUntilFailure===0) {
            this.state.cellsUntilFailure=null;throw Error('Interrupted partial range write');
          }
        });
      });
      if (this.state.failWrite) { this.state.failWrite=false;throw Error('Interrupted after write'); }
    };
    return {
      getValues:()=>Array.from({length:h},(_,i)=>Array.from({length:w},(_,j)=>this.cell(r+i,c+j).value)),
      getFormulas:()=>Array.from({length:h},(_,i)=>Array.from({length:w},(_,j)=>this.cell(r+i,c+j).formula)),
      setValues:values=>write(values),
      setValue:value=>{assert.equal(h*w,1);write([[value]]);},
      setFormula:formula=>{assert.equal(h*w,1);write([['calculated']],[[formula]]);},
    };
  }
}
function fixture(source = fs.readFileSync(__dirname+'/collector.gs','utf8')) {
  const state={writes:0,failWrite:false,cellsUntilFailure:null,triggers:[],locked:false,timezone:'Europe/Zagreb'};
  const sheets=new Map();
  const add=(name,rows=1000,cols=26)=>{const s=new Sheet(name,state,rows,cols);sheets.set(name,s);return s;};
  const raw=add('Installations',1,3),summary=add('Summary'),countries=add('Countries');
  ['Installation hash','Last active (UTC)','Version'].forEach((v,i)=>raw.put(1,i+1,v));
  summary.put(1,1,0,'=COUNTIFS(Installations!B2:B20001,">="&TODAY(),Installations!B2:B20001,"<="&TODAY())');
  summary.put(2,1,37,'=SUM(Historical!B2:B30)');
  const book={getSheetByName:name=>sheets.get(name),insertSheet:name=>{assert.ok(!sheets.has(name));state.writes++;return add(name);},
    setSpreadsheetTimeZone:zone=>{state.timezone=zone;state.writes++;}};
  const trigger={timeBased:()=>trigger,everyDays:()=>trigger,atHour:()=>trigger,create:()=>{state.writes++;state.triggers.push({getHandlerFunction:()=> 'maintenance'});}};
  const sandbox={module:{exports:{}},Date,JSON,
    PropertiesService:{getScriptProperties:()=>({getProperty:()=> 'private-sheet-id'})},
    SpreadsheetApp:{openById:()=>book,flush:()=>{}},
    ScriptApp:{getProjectTriggers:()=>state.triggers,newTrigger:()=>trigger},
    LockService:{getScriptLock:()=>({tryLock:()=>{state.locked=true;return true;},hasLock:()=>state.locked,releaseLock:()=>{state.locked=false;}})},
    ContentService:{MimeType:{JSON:'application/json'},createTextOutput:text=>({setMimeType:()=>JSON.parse(text)})},
    Utilities:{DigestAlgorithm:{SHA_256:'sha256'},computeDigest:(_,value)=>[...crypto.createHash('sha256').update(value).digest()],formatDate:date=>date.toISOString().slice(0,10)},
  };
  vm.runInNewContext(source,sandbox);
  const values=(sheet,width)=>sheet ? sheet.getRange(1,1,Math.max(1,sheet.getLastRow()),width).getValues() : [];
  return {sandbox,raw,sheets,add,state,
    snapshot:()=>JSON.stringify({timezone:state.timezone,triggers:state.triggers.map(t=>t.getHandlerFunction()),
      sheets:[...sheets].map(([name,s])=>({name,maxRows:s.maxRows,maxColumns:s.maxColumns,frozenRows:s.frozenRows,cells:[...s.cells].sort()}))}),
    summary:()=>summary.getDataRange().getFormulas(),reporting:()=>values(sheets.get('Reporting'),2).slice(0,6),
    countries:()=>values(countries,4),rows:()=>values(raw,Math.min(raw.getMaxColumns(),16)),
    status:()=>summary.cell(9,2).value,triggers:()=>state.triggers,locked:()=>state.locked,
    seed:rows=>{
      for(const key of raw.cells.keys()) if(+key.split(',')[0]>1) raw.cells.delete(key);
      raw.maxRows=Math.max(raw.maxRows,rows.length+1);
      rows.forEach((row,i)=>{raw.maxColumns=Math.max(raw.maxColumns,row.length);row.forEach((v,j)=>raw.put(i+2,j+1,v));});
    },
    failAfterWrite:()=>{state.failWrite=true;},failAfterCells:n=>{state.cellsUntilFailure=n;},
  };
}
module.exports={fixture,Sheet};
