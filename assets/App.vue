<template>
    <v-app v-cloak>
        <v-navigation-drawer
            bottom
            v-model="drawer"
            app
        >
            <v-list>
                <v-list-item
                    v-for="(item,i) in leftMenuItems"
                    :key="i"
                    active-color="light-blue-lighten-2"
                    :value="item"
                    :to="item.value"
                >
                    <v-list-item-title v-text="item.title"></v-list-item-title>
                </v-list-item>
            </v-list>
        </v-navigation-drawer>

        <v-app-bar short app>
            <v-app-bar-nav-icon variant="text" @click="drawer = !drawer"></v-app-bar-nav-icon>
            <v-toolbar-title>Profit of Exile</v-toolbar-title>
        </v-app-bar>

        <v-main>
            <v-container fluid>
                <router-view></router-view>
            </v-container>
        </v-main>
    </v-app>
</template>

<script>
export default {
    name: 'App',
    data() {
        return {
            drawer: null,
            leftMenuItems: [
                {
                    title: "Home",
                    value: '/'
                },
                {
                    title: "Compose Strategies",
                    value: '/compose'
                },
                // {
                //     title: "Shaper Rotation",
                //     value: '/rotation/full-shaper-invitation'
                // },
            ]
        }
    }
}
</script>
