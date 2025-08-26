import { useEffect } from "react";

const isDark = () =>
  localStorage.theme === "dark" ||
  (!("theme" in localStorage) &&
    window.matchMedia("(prefers-color-scheme: dark)").matches);

const setDark = () => {
  const dark = isDark();
  document.documentElement.classList[dark ? "add" : "remove"]("dark");
};

const toggleDark = () => {
  const dark = isDark();
  document.documentElement.classList[dark ? "remove" : "add"]("dark");
};

const useDarkMode = () => {
  useEffect(() => {
    setDark();

    const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
    systemDark.addEventListener("change", setDark);

    return () => {
      systemDark.removeEventListener("change", setDark);
    };
  }, []);

  return { isDark, toggleDark };
};

export { useDarkMode, isDark, toggleDark, setDark };
