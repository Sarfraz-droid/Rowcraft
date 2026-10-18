'use strict';
// Deliberately small, read-only SQL subset for the local sample tables.
window.runSampleQuery = function (sql, tables) {
  const tokens = [];
  const lexer = /\s+|--[^\n]*|\/\*[\s\S]*?\*\/|'(?:''|[^'])*'|[a-z_][a-z_0-9]*|-?\d+(?:\.\d+)?|>=|<=|<>|!=|[=<>*,.;]/giy;
  let offset = 0;
  while (offset < sql.length) {
    lexer.lastIndex = offset;
    const match = lexer.exec(sql);
    if (!match) throw new Error('Unsupported SQL. Try SELECT columns FROM table WHERE column = value ORDER BY column LIMIT 10.');
    offset = lexer.lastIndex;
    if (!/^\s|^--|^\/\*/.test(match[0])) tokens.push(match[0]);
  }
  let index = 0;
  const peek = word => tokens[index]?.toUpperCase() === word;
  function take(word) { if (!peek(word)) throw new Error(`Expected ${word}. This demo supports SELECT, WHERE, ORDER BY and LIMIT.`); index++; }
  function identifier() { const token = tokens[index++]; if (!/^[a-z_][a-z_0-9]*$/i.test(token || '')) throw new Error('Expected a table or column name.'); return token.toLowerCase(); }
  take('SELECT');
  let columns = [];
  if (peek('*')) { index++; columns = null; }
  else { columns.push(identifier()); while (peek(',')) { index++; columns.push(identifier()); } }
  take('FROM');
  let table = identifier();
  if (peek('.')) { if (table !== 'public') throw new Error('Use the public schema for sample tables.'); index++; table = identifier(); }
  if (!Object.hasOwn(tables, table)) throw new Error(`Unknown table "${table}". Try customers, orders or products.`);
  const dataset = tables[table];
  function column() { const name = identifier(); if (!dataset.columns.includes(name)) throw new Error(`Unknown column "${name}" in ${table}. Available: ${dataset.columns.join(', ')}.`); return name; }
  columns ??= dataset.columns.slice();
  for (const name of columns) if (!dataset.columns.includes(name)) throw new Error(`Unknown column "${name}" in ${table}. Available: ${dataset.columns.join(', ')}.`);
  let rows = dataset.rows.map(row => Object.fromEntries(dataset.columns.map((name, i) => [name, dataset.types[i] === 'numeric' || dataset.types[i] === 'integer' ? Number(row[i]) : row[i]])));
  if (peek('WHERE')) {
    index++;
    do {
      const name = column(); const operator = tokens[index++];
      if (!['=', '!=', '<>', '>', '<', '>=', '<='].includes(operator)) throw new Error('WHERE supports =, !=, <>, >, <, >= and <= comparisons.');
      const literal = tokens[index++];
      if (!literal || !/^(?:'(?:''|[^'])*'|-?\d+(?:\.\d+)?)$/.test(literal)) throw new Error('Use a quoted string or a number in WHERE.');
      const value = literal.startsWith("'") ? literal.slice(1,-1).replaceAll("''", "'") : Number(literal);
      rows = rows.filter(row => { const actual = row[name]; switch(operator) { case '=': return actual === value; case '!=': case '<>': return actual !== value; case '>': return actual > value; case '<': return actual < value; case '>=': return actual >= value; case '<=': return actual <= value; } });
      if (!peek('AND')) break; index++;
    } while (true);
  }
  if (peek('ORDER')) {
    index++; take('BY'); const name = column(); let direction = 1;
    if (peek('DESC')) { index++; direction = -1; } else if (peek('ASC')) index++;
    rows.sort((a,b) => direction * (a[name] < b[name] ? -1 : a[name] > b[name] ? 1 : 0));
  }
  if (peek('LIMIT')) { index++; const limit = tokens[index++]; if (!/^\d+$/.test(limit || '') || !Number.isSafeInteger(Number(limit))) throw new Error('LIMIT must be a non-negative whole number.'); rows = rows.slice(0, Number(limit)); }
  if (peek(';')) index++;
  if (index !== tokens.length) throw new Error('Unsupported SQL. Run one SELECT with optional WHERE, ORDER BY and LIMIT.');
  return { table, columns, rows: rows.map(row => columns.map(name => row[name])) };
};
