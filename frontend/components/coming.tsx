import Image from "next/image";

export default function Coming() {
  return (
    <main className="flex grow flex-col">
      <div className="flex  flex-1 flex-col justify-center">
        <div className="flex justify-center">
          <Image
            src="/images/coming_dark.svg"
            alt="desk"
            width={406}
            height={438}
            className="w-60 md:w-96 h-auto dark:block hidden"
          />
          <Image
            src="/images/coming.svg"
            alt="desk"
            width={406}
            height={438}
            className="w-60 md:w-96 h-auto dark:hidden block"
          />
        </div>
        <div className="flex justify-center">
          <h1 className="dark:text-slate-300 font-mono text-slate-700 mt-8 text-3xl font-semibold text-center">
            Coming Soon
          </h1>
        </div>
      </div>
    </main>
  );
}
