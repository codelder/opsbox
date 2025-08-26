import Image from "next/image";
import LogoText from "@/components/text";
export default function Logo({ dark = false, ...props }) {
  return (
    <div {...props} className="flex items-center text-black dark:text-white">
      <Image
        className="h-8 w-auto"
        src="/images/logo.svg"
        alt="Offic"
        width={32}
        height={32}
      />
      <LogoText className="text-black dark:text-white mt-0.5 ml-2" />
    </div>
  );
}
