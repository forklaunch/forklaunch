/* eslint-disable @typescript-eslint/no-unused-vars */
import { describe, expect, it } from 'vitest';
import { extractArgumentNames } from '../src/extractArgumentNames';

describe('extractArgumentNames', () => {
  it('should extract single argument name', () => {
    const fn = (arg1: string) => {};
    expect(extractArgumentNames(fn)).toEqual(['arg1']);
  });

  it('should extract multiple argument names', () => {
    const fn = (arg1: string, arg2: number, arg3: boolean) => {};
    expect(extractArgumentNames(fn)).toEqual(['arg1', 'arg2', 'arg3']);
  });

  it('should handle no arguments', () => {
    const fn = () => {};
    expect(extractArgumentNames(fn)).toEqual([]);
  });

  it('should handle object destructuring', () => {
    const fn = ({ name, age }: { name: string; age: number }) => {};
    expect(extractArgumentNames(fn)).toEqual(['{name,age}']);
  });

  it('should handle nested object destructuring', () => {
    const fn = ({
      user: { name, age }
    }: {
      user: { name: string; age: number };
    }) => {};
    expect(extractArgumentNames(fn)).toEqual(['{user:{name,age}}']);
  });

  it('should handle multiple arguments with destructuring', () => {
    const fn = (
      { name }: { name: string },
      age: number,
      { active }: { active: boolean }
    ) => {};
    expect(extractArgumentNames(fn)).toEqual(['{name}', 'age', '{active}']);
  });

  it('should handle whitespace in arguments', () => {
    const fn = (arg1: string, arg2: number) => {};
    expect(extractArgumentNames(fn)).toEqual(['arg1', 'arg2']);
  });

  it('should handle arguments with newlines and spaces', () => {
    const fn = (
      arg1: string,

      arg2: number,
      arg3: boolean
    ) => {};
    expect(extractArgumentNames(fn)).toEqual(['arg1', 'arg2', 'arg3']);
  });

  it('should handle arguments with tabs and multiple spaces', () => {
    const fn = (arg1: string, arg2: number, arg3: boolean) => {};
    expect(extractArgumentNames(fn)).toEqual(['arg1', 'arg2', 'arg3']);
  });

  it('should handle multiline object destructuring', () => {
    const fn = ({ name, age }: { name: string; age: number }) => {};
    expect(extractArgumentNames(fn)).toEqual(['{name,age}']);
  });

  it('should handle multiline object destructuring with multiple args', () => {
    const fn = (
      {
        name,
        age
      }: {
        name: string;
        age: number;
      },
      resolve: () => void,
      context: Record<string, unknown>
    ) => {};
    expect(extractArgumentNames(fn)).toEqual([
      '{name,age}',
      'resolve',
      'context'
    ]);
  });

  it('should return empty array for invalid function string', () => {
    const invalidFn = { toString: () => 'not a function' };
    expect(extractArgumentNames(invalidFn)).toEqual([]);
  });
});
