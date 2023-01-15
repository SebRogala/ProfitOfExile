import {createRouter, createWebHashHistory} from "vue-router";

import Home from "./views/Home";
import FullShaperInvitation from "./views/Rotation/FullShaperInvitation";

const routes = [
    {path: '/', name: "home", component: Home},
    {path: '/rotation/full-shaper-invitation', name: "rotation/full-shaper-invitation", component: FullShaperInvitation},
]

const router = createRouter({
    history: createWebHashHistory(),
    routes,
})

export default router
