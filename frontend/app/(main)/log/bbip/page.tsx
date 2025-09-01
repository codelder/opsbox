"use client";

import { Datepicker, Select } from "flowbite-react";
export default function Example() {
  return (
    <div className="h-full">
      <div className="inset-y-0 z-50 flex w-96 flex-col p-4 h-full">
        <div className="flex flex-1 flex-col gap-y-5 overflow-y-auto  bg-white dark:bg-gray-900 px-6">
          <h5
            id="drawer-label"
            className="inline-flex items-center mb-6 text-sm font-semibold text-gray-500 uppercase dark:text-gray-400"
          >
            检索条件
          </h5>
          <form className="flex max-w-sm flex-col gap-8 text-sm">
            <div>
              <div className="mb-2 block  dark:text-gray-200">
                <label htmlFor="name">交易日期</label>
              </div>
              <Datepicker />
            </div>
            <div>
              <div className="mb-2 block dark:text-gray-200">
                <label htmlFor="name">微服务</label>
              </div>
              <Select />
            </div>
          </form>
        </div>
      </div>

      <main className="py-10 pl-72">
        <div className="px-8">{/* Your content */}</div>
      </main>
    </div>
  );
}
