"use client";

import Header from "@/app/(main1)/header";
import SideNav from "@/app/(main1)/sidenav";
import DarkModeToggle from "@/components/dark-toggle";
import { Dialog, DialogPanel, Transition, TransitionChild } from "@headlessui/react";
import { Bars3Icon, XMarkIcon } from "@heroicons/react/24/solid";
import { Fragment, useState } from "react";
import PerfectScrollbar from "react-perfect-scrollbar";
import "react-perfect-scrollbar/dist/css/styles.css";

export default function Layout({ children }) {
  const [sidebarOpen, setSidebarOpen] = useState(false);

  return (
    <div className={`h-full`}>
      <Transition show={sidebarOpen} as={Fragment}>
        <Dialog as="div" className={`relative z-50 xl:hidden`} onClose={setSidebarOpen}>
          <TransitionChild
            as={Fragment}
            enter={`transition-opacity ease-linear duration-300`}
            enterFrom={`opacity-0`}
            enterTo={`opacity-100`}
            leave={`transition-opacity ease-linear duration-300`}
            leaveFrom={`opacity-100`}
            leaveTo={`opacity-0`}
          >
            <div className={`fixed inset-0 bg-slate-900/80`} />
          </TransitionChild>

          <div className={`flex1 fixed inset-0`}>
            <TransitionChild
              as={Fragment}
              enter={`transition ease-in-out duration-300 transform`}
              enterFrom={`-translate-x-full`}
              enterTo={`translate-x-0`}
              leave={`transition ease-in-out duration-300 transform`}
              leaveFrom={`translate-x-0`}
              leaveTo={`-translate-x-full`}
            >
              <DialogPanel className={`relative mr-16 flex w-full max-w-xs flex-1`}>
                <TransitionChild
                  as={Fragment}
                  enter={`ease-in-out duration-300`}
                  enterFrom={`opacity-0`}
                  enterTo={`opacity-100`}
                  leave={`ease-in-out duration-300`}
                  leaveFrom={`opacity-100`}
                  leaveTo={`opacity-0`}
                >
                  <div className={`absolute top-0 left-full flex w-16 justify-center pt-5`}>
                    <button type="button" className={`-m-2.5 p-2.5`} onClick={() => setSidebarOpen(false)}>
                      <span className={`sr-only`}>Close sidebar</span>
                      <XMarkIcon className={`h-6 w-6 text-white`} aria-hidden="true" />
                    </button>
                  </div>
                </TransitionChild>
                {/* Sidebar component, swap this element with another sidebar if you like */}
                <div />
              </DialogPanel>
            </TransitionChild>
          </div>
        </Dialog>
      </Transition>
      <div className={`hidden xl:fixed xl:inset-y-0 xl:z-50 xl:flex xl:w-72 xl:flex-col`}>
        <SideNav />
      </div>
      <PerfectScrollbar className="xl:pl-72 h-full flex flex-col">
        {/* Sticky search header */}
        <div className="sticky top-0 z-40 flex h-16 shrink-0 items-center gap-x-6 border-b border-black/5 dark:border-white/5 bg-slate-100 dark:bg-slate-900 px-4 shadow-sm sm:px-6 lg:px-8">
          <button
            type="button"
            className="-m-2.5 p-2.5 text-black dark:text-white xl:hidden"
            onClick={() => setSidebarOpen(true)}
          >
            <span className="sr-only">Open sidebar</span>
            <Bars3Icon className="h-5 w-5" aria-hidden="true" />
          </button>
          <Header />
          <DarkModeToggle />
        </div>
        {children}
      </PerfectScrollbar>
    </div>
  );
}
