/** Production entry point (Heroku): Express on Bun. */
import { createApp, warmSelections } from "./server/app";

const port = Number(process.env.PORT ?? 3000);
createApp().listen(port, () => console.log(`Altim web en ligne sur http://localhost:${port}`));
if (process.env.NODE_ENV === "production") warmSelections();
