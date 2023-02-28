<template>
    <v-dialog
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
                Save / Load
            </v-btn>
        </template>
        <v-card>
            <v-card-title>
                <span class="text-h5">Manage storing strategies</span>
            </v-card-title>
            <v-card-text>
                <v-container>
                    <v-table
                        class="mb-4"
                        density="compact"
                    >
                        <thead>
                        <tr>
                            <th class="text-left">
                                Name
                            </th>
                            <th class="text-left">
                                Actions
                            </th>
                        </tr>
                        </thead>
                        <tbody>
                        <tr
                            v-for="strategy in strategies"
                        >
                            <td>{{ strategy }}</td>
                            <td>
                                <v-btn
                                    variant="outlined"
                                    class="mr-4"
                                    size="small"
                                    color="info"
                                    @click="load(strategy)"
                                >
                                    Load
                                </v-btn>

                                <v-btn
                                    v-if="composedStrategy.length"
                                    class="ml-1"
                                    variant="text"
                                    size="small"
                                    color="warning"
                                    @click="overwrite(strategy)"
                                >
                                    Overwrite
                                </v-btn>

                                <v-btn
                                    class="ml-1"
                                    variant="text"
                                    size="small"
                                    color="error"
                                    @click="remove(strategy)"
                                >
                                    Delete
                                </v-btn>
                            </td>
                        </tr>
                        </tbody>
                    </v-table>

                    <v-form ref="form" v-if="composedStrategy.length">
                        <span class="text-h6">Save strategy</span>
                        <v-text-field
                            class="mt-2"
                            label="Strategy name"
                            variant="outlined"
                            density="compact"
                            v-model="newStrategyName"
                            :rules="[
                                    v => !!v || 'Strategy name is required'
                                  ]"
                        ></v-text-field>
                        <v-btn
                            color="success"
                            variant="outlined"
                            @click="save"
                        >
                            Save
                        </v-btn>
                    </v-form>
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
    name: "SaveLoadStrategy",
    props: {
        composedStrategy: Array,
    },
    emits: ['loaded', 'saved'],
    data() {
        return {
            newStrategyName: "",
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
        },
        remove(name) {
            this.$storage.deleteStrategy(name);
            this.getStrategies();
        },
        async overwrite(name) {
            this.dialog = false;
            this.$storage.saveStrategy(name, this.composedStrategy);
            this.$emit('saved');
        },
        async save() {
            if (!this.composedStrategy.length) {
                this.dialog = false;
                return;
            }

            const {valid} = await this.$refs.form.validate()

            if (!valid) {
                return;
            }

            this.dialog = false;
            this.$storage.saveStrategy(this.newStrategyName, this.composedStrategy);
            this.newStrategyName = '';
            this.$emit('saved');
        }
    }
}
</script>
