// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { SchemaMap } from './SchemaMap';
import { tableId } from '../workbench-model';
import { saveMongoLinks } from '../mongo-relationships';
const source = {name:'test',dialect:'mongo',host:'localhost',ssh_tunnel:null,color:null};
const catalog = {database:'db',schemas:[{name:'db',tables:['users','posts'].map(name => ({name,kind:'Table' as const,columns:(name === 'users' ? ['_id'] : ['_id','userId']).map(name => ({name,native_type:'objectId',nullable:false}))}))}]};
const props = {source,catalog,connected:true,busy:false,onRefresh:vi.fn(),onOpenTable:vi.fn()};
afterEach(() => {cleanup();localStorage.clear();});
it('draws inferred links with dashed provenance and lets users disable them', async () => {
  const {container} = render(<SchemaMap {...props} />);
  await screen.findByText(/test · 2 of 2 collections · 1 reference links/);
  const path = container.querySelector('path[stroke-dasharray="6 4"]');
  expect(path?.textContent).toContain('Inferred reference');
  expect(path?.textContent).toContain('not been verified');
  fireEvent.click(screen.getByLabelText('Infer references'));
  await waitFor(() => expect(container.querySelector('path[stroke-dasharray="6 4"]')).toBeNull());
});
it('loads saved manual links, removes them persistently, and isolates sources', async () => {
  const key = `rowcraft.mongo-references.v1:${JSON.stringify(['test','localhost'])}`;
  saveMongoLinks(key,[{from:tableId('db','posts'),field:'userId',to:tableId('db','users'),targetField:'_id'}]);
  const {container,rerender} = render(<SchemaMap {...props} />);
  await waitFor(() => expect(container.querySelector('path[stroke-dasharray="2 3"]')).toBeTruthy());
  expect(container.querySelector('path[stroke-dasharray="6 4"]')).toBeNull();
  fireEvent.click(screen.getByText(/Manage references/));
  fireEvent.click(screen.getByRole('button',{name:/Remove reference/}));
  expect(localStorage.getItem(key)).toBe('[]');
  await waitFor(() => expect(container.querySelector('path[stroke-dasharray="6 4"]')).toBeTruthy());
  rerender(<SchemaMap {...props} source={{...source,name:'other'}} />);
  expect(screen.getByText(/Manage references · 0 manual/)).toBeTruthy();
});
