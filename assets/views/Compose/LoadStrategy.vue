<template>
    <v-dialog
        style="width: 600px"
        v-model="dialog"
        persistent
    >
        <template v-slot:activator="{ props }">
            <v-btn
                class="ml-4"
                color="primary"
                v-bind="props"
                @click="getStrategies"
            >
                Load
            </v-btn>
        </template>
        <v-card>
            <v-card-title>
                <span class="text-h5">Load strategy</span>
            </v-card-title>
            <v-card-text>
                <v-container>
                    <v-btn
                        v-for="strategy in strategies"
                        class="ma-1"
                        variant="text"
                        color="info"
                        @click="load(strategy)"
                    >
                        {{ strategy }}
                    </v-btn>
                </v-container>
            </v-card-text>
            <v-card-actions>
                <v-spacer></v-spacer>
                <v-btn
                    color="blue-darken-1"
                    variant="text"
                    @click="dialog = false"
                >
                    Close
                </v-btn>
            </v-card-actions>
        </v-card>
    </v-dialog>
</template>

<script>
export default {
    name: "LoadStrategy",
    emits: ['loaded'],
    data() {
        return {
            dialog: false,
            strategies: []
        }
    },
    methods: {
        getStrategies() {
            this.strategies = this.$storage.getStrategyNames();
        },
        load(name) {
            this.dialog = false;
            this.$emit('loaded', this.$storage.getStrategy(name));
        }
    }
}
</script>
