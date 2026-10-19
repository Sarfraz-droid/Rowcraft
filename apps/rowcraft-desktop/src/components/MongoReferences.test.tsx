// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MongoReferences } from './MongoReferences';
import { tableId, type CatalogTable } from '../workbench-model';
import { chooseOption } from '../test-utils/select';
afterEach(cleanup);
const tables: CatalogTable[] = ['posts','users'].map(name => ({id:tableId('db',name),schema:'db',table:{name,kind:'Table',columns:['_id','authorId'].map(name => ({name,native_type:'objectId',nullable:false}))}}));
it('adds an explicit source/target reference and defaults the target to _id', async () => {
  const onChange = vi.fn();
  render(<MongoReferences tables={tables} links={[]} onChange={onChange} error="" />);
  fireEvent.click(screen.getByText(/Manage references/));
  expect(screen.getByRole('button',{name:'Add reference'}).hasAttribute('disabled')).toBe(true);
  await chooseOption(screen.getByLabelText('Reference source collection'),'db.posts');
  await chooseOption(screen.getByLabelText('Reference source field'),'authorId');
  await chooseOption(screen.getByLabelText('Reference target collection'),'db.users');
  fireEvent.click(screen.getByRole('button',{name:'Add reference'}));
  expect(onChange).toHaveBeenCalledWith([{from:tables[0]!.id,field:'authorId',to:tables[1]!.id,targetField:'_id'}]);
});
it('lets users remove stale links and reports storage errors', () => {
  const onChange = vi.fn();
  render(<MongoReferences tables={[]} links={[{from:'old',field:'authorId',to:'users',targetField:'_id'}]} onChange={onChange} error="Could not save references locally." />);
  fireEvent.click(screen.getByText(/Manage references/));
  expect(screen.getByText(/Not in current metadata/)).toBeTruthy();
  expect(screen.getByRole('alert').textContent).toContain('Could not save');
  fireEvent.click(screen.getByRole('button',{name:/Remove reference/}));
  expect(onChange).toHaveBeenCalledWith([]);
});
