// "use client";

import { useEffect, useState } from "react";
import { setDark } from "@/hook/dark-mode";

declare type Mode = "auto" | "dark" | "light";

export default function DarkModeToggle() {
  const [mode, setMode] = useState<Mode>("auto");

  useEffect(() => {
    setMode(localStorage.theme || "auto");
  }, []);

  const toggleMode = (mode: Mode) => {
    if (mode === "auto") {
      localStorage.setItem("theme", "light");
      setMode("light");
    } else if (mode === "dark") {
      localStorage.removeItem("theme");
      setMode("auto");
    } else {
      localStorage.setItem("theme", "dark");
      setMode("dark");
    }
    setDark();
  };

  return (
    <div className="flex items-center justify-center">
      {/* <span className="dark:text-white">{mode}</span> */}
      <button
        className="p-1 text-gray-400 rounded-md hover:text-gray-500 dark:hover:text-gray-300 text-xl"
        onClick={() => toggleMode(mode)}
      >
        <i
          className={
            mode === "auto"
              ? "fa fa-circle-a"
              : mode === "dark"
              ? "fa-regular fa-moon-stars"
              : "fa-regular fa-sun-bright"
          }
        ></i>
      </button>
    </div>
  );
}
