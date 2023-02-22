<template>
    <v-dialog
        style="width: 400px"
        v-model="dialog"
        persistent
    >
        <template v-slot:activator="{ props }">
            <v-btn
                class="ml-10"
                color="primary"
                v-bind="props"
            >
                Save
            </v-btn>
        </template>
        <v-card>
            <v-card-title>
                <span class="text-h5">Strategy name</span>
            </v-card-title>
            <v-card-text>
                <v-container>
                    <v-form ref="form">
                        <v-text-field
                            label="Name"
                            variant="outlined"
                            density="compact"
                            v-model="name"
                            :rules="[
                                    v => !!v || 'Strategy name is required'
                                  ]"
                        ></v-text-field>
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
                <v-btn
                    color="blue-darken-1"
                    variant="text"
                    @click="save"
                >
                    Confirm
                </v-btn>
            </v-card-actions>
        </v-card>
    </v-dialog>
</template>

<script>
export default {
    name: "SaveStrategy",
    props: {
        strategies: Array,
    },
    data() {
        return {
            name: "",
            dialog: false,
        }
    },
    methods: {
        async save() {
            if (!this.strategies.length) {
                this.dialog = false;
                return;
            }

            const {valid} = await this.$refs.form.validate()

            if (!valid) {
                return;
            }

            this.$storage.saveStrategy(this.name, this.strategies);
            this.dialog = false;
        }
    }
}
</script>
