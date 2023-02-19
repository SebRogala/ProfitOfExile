import {createRouter, createWebHashHistory} from "vue-router";

import Home from "./views/Home";
import FullShaperInvitation from "./views/Rotation/FullShaperInvitation";
import ComposeStrategies from "./views/Compose/ComposeStrategies";

const routes = [
    {path: '/', name: "home", component: Home},
    {path: '/rotation/full-shaper-invitation', name: "rotation/full-shaper-invitation", component: FullShaperInvitation},
    {path: '/compose', name: "compose", component: ComposeStrategies},
]

const router = createRouter({
    history: createWebHashHistory(),
    routes,
})

export default router
