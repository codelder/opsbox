import Logo from "@/components/logo";
import { path_match } from "@/utils/utils";
import { DocumentMagnifyingGlassIcon } from "@heroicons/react/24/solid";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";
import PerfectScrollbar from "react-perfect-scrollbar";

const teams = [
  { id: 1, name: "Planetaria", href: "#", initial: "P", current: false },
  { id: 2, name: "Protocol", href: "#", initial: "P", current: false },
  { id: 3, name: "Tailwind Labs", href: "#", initial: "T", current: false },
];

function classNames(...classes: string[]) {
  return classes.filter(Boolean).join(" ");
}

const Nav = ({ name, href, icon: Icon, current }) => (
  <Link
    href={href}
    className={classNames(
      current
        ? "bg-sky-700 text-white hover:shadow-2xl"
        : "text-slate-700 dark:text-slate-400 hover:bg-white dark:hover:bg-slate-700 hover:shadow-lg",
      "group flex gap-x-3 rounded-md p-2 text-sm leading-6",
    )}
  >
    <Icon className="h-6 w-6 shrink-0" aria-hidden="true" />
    {name}
  </Link>
);

export default function SideNav() {
  const [navigation, setNavigation] = useState([
    {
      name: "日志检索",
      href: "/log",
      icon: DocumentMagnifyingGlassIcon,
      current: false,
    },
    {
      name: "日志查询",
      href: "/bbiplog",
      icon: DocumentMagnifyingGlassIcon,
      current: false,
    },
  ]);
  const pathname = usePathname();

  useEffect(() => {
    const updatedNavigation = navigation.map((item) => ({
      ...item,
      current: path_match(pathname, item.href),
    }));
    setNavigation(updatedNavigation);
  }, [pathname]);

  return (
    <PerfectScrollbar className="flex-grow flex flex-col gap-y-5 bg-slate-100 dark:bg-slate-900 px-6 ring-1 ring-black/10 dark:ring-white/10">
      <div className="flex h-16 shrink-0 items-center">
        <Logo />
      </div>
      <nav className="flex flex-1 flex-col">
        <ul role="list" className="flex flex-1 flex-col gap-y-7">
          <li>
            <ul role="list" className="-mx-2 space-y-1">
              {navigation.map((item) => (
                <li className={`my-3`} key={item.name}>
                  <Nav {...item} />
                </li>
              ))}
            </ul>
          </li>
          {/*<li>*/}
          {/*  <div className="text-xs font-medium leading-6 text-slate-600 dark:text-slate-400">常 用</div>*/}
          {/*  <ul role="list" className="-mx-2 mt-2 space-y-1">*/}
          {/*    {teams.map((team) => (*/}
          {/*      <li key={team.name}>*/}
          {/*        <a*/}
          {/*          href={team.href}*/}
          {/*          className={classNames(*/}
          {/*            team.current*/}
          {/*              ? "bg-sky-700 text-white hover:shadow-2xl"*/}
          {/*              : "text-slate-700 dark:text-slate-300 hover:bg-white dark:hover:bg-slate-700 hover:shadow-lg",*/}
          {/*            "group flex gap-x-3 rounded-md p-2 text-sm leading-6",*/}
          {/*          )}*/}
          {/*        >*/}
          {/*          <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-lg border border-slate-300 dark:border-slate-700  bg-slate-200 dark:bg-slate-800  text-[0.625rem] font-medium text-slate-600 dark:text-slate-400  group-hover:text-black dark:group-hover:text-white ">*/}
          {/*            {team.initial}*/}
          {/*          </span>*/}
          {/*          <span className="truncate">{team.name}</span>*/}
          {/*        </a>*/}
          {/*      </li>*/}
          {/*    ))}*/}
          {/*  </ul>*/}
          {/*</li>*/}
          <li className="-mx-6 mt-auto">
            <Link
              href="/profile"
              className={classNames(
                path_match(pathname, "/profile")
                  ? "bg-sky-700 text-white"
                  : "text-black dark:text-white  hover:bg-slate-200 dark:hover:bg-slate-800",
                "flex items-center gap-x-4 px-6 py-3 text-sm font-semibold leading-6",
              )}
            >
              <img
                className="h-8 w-8 rounded-full bg-slate-200 dark:bg-slate-800"
                src="https://images.unsplash.com/photo-1472099645785-5658abf4ff4e?ixlib=rb-1.2.1&ixid=eyJhcHBfaWQiOjEyMDd9&auto=format&fit=facearea&facepad=2&w=256&h=256&q=80"
                alt=""
              />
              <span className="sr-only">Your profile</span>
              <span aria-hidden="true">Tom Cook</span>
            </Link>
          </li>
        </ul>
      </nav>
    </PerfectScrollbar>
  );
}
