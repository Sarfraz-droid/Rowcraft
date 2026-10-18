const {test} = require('node:test');
const assert = require('node:assert/strict');
const {readFileSync} = require('node:fs');
const {runInNewContext} = require('node:vm');
const window = {};
runInNewContext(readFileSync(`${__dirname}/../demo-query.js`, 'utf8'), {window});
const tables = { products: {columns:['name','price'], types:['text','numeric'], rows:[["Writer's desk",'200'],['Lamp','30'],['Stand','120']]} };
const query = sql => JSON.parse(JSON.stringify(window.runSampleQuery(sql,tables)));
test('edited query projects, filters numerically, orders and limits results',()=>{
 assert.deepEqual(query('SELECT name FROM public.products WHERE price > 100 ORDER BY price DESC LIMIT 1;').rows,[["Writer's desk"]]);
});
test('SQL strings preserve escaped quotes and statement punctuation',()=>{
 assert.deepEqual(query("SELECT * FROM products WHERE name = 'Writer''s desk';").rows,[["Writer's desk",200]]);
});
test('AND conditions, comments and empty results',()=>{
 assert.equal(query('-- example\nSELECT * FROM products WHERE price >= 30 AND price < 100').rows.length,1);
 assert.equal(query('SELECT * FROM products LIMIT 0').rows.length,0);
});
test('invalid columns and tables give actionable errors',()=>{
 assert.throws(()=>query('SELECT unknown FROM products'),/Unknown column/);
 assert.throws(()=>query('SELECT * FROM secrets'),/Unknown table/);
});
test('mutations, multiple statements and unsupported syntax fail visibly',()=>{
 for(const sql of ['DELETE FROM products','SELECT * FROM products; DELETE FROM products','SELECT * FROM products LIMIT -1','SELECT * FROM products WHERE price =']) assert.throws(()=>query(sql));
 assert.equal(query('SELECT * FROM products').rows.length,3);
});
